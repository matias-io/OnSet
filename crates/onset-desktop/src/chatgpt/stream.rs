//! Reading a streamed Responses API answer: the output text, and every HTTPS page the model
//! consulted (search sources, opened pages, URL citations). Only a completed response counts;
//! an interrupted, failed or incomplete one is an error, so nothing partial is ever saved.
use std::collections::BTreeSet;
use std::io::BufRead;

use serde_json::Value;

/// The completed answer and the pages it consulted.
#[derive(Debug, Default, PartialEq)]
pub struct Completed {
    pub text: String,
    pub citations: BTreeSet<String>,
}

/// An HTTPS URL without credentials, as written; anything else is ignored.
fn add_url(citations: &mut BTreeSet<String>, value: Option<&Value>) {
    let Some(s) = value.and_then(Value::as_str) else {
        return;
    };
    if let Ok(url) = tauri::Url::parse(s)
        && url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
    {
        citations.insert(s.to_owned());
    }
}

fn collect_item(citations: &mut BTreeSet<String>, item: &Value) {
    for source in item
        .pointer("/action/sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        add_url(citations, source.get("url"));
    }
    // An opened or searched-in page can be absent from the search source list.
    let action = item.pointer("/action/type").and_then(Value::as_str);
    if matches!(action, Some("open_page" | "find_in_page")) {
        add_url(citations, item.pointer("/action/url"));
    }
    for content in item
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for annotation in content
            .get("annotations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if annotation.get("type").and_then(Value::as_str) == Some("url_citation") {
                add_url(citations, annotation.get("url"));
            }
        }
    }
}

/// Reads server-sent events until `response.completed`. `progress` gets each text delta.
pub fn read(body: impl BufRead, mut progress: impl FnMut(&str)) -> Result<Completed, String> {
    let mut out = Completed::default();
    let mut data: Vec<String> = Vec::new();
    let mut lines = body.lines();
    loop {
        let line = match lines.next() {
            Some(Ok(l)) => l,
            Some(Err(e)) => return Err(format!("ChatGPT response stream failed: {e}")),
            None => return Err("Interrupted ChatGPT response; no recommendation saved".into()),
        };
        let line = line.strip_suffix('\r').unwrap_or(&line);
        if !line.is_empty() {
            if let Some(d) = line.strip_prefix("data:") {
                data.push(d.trim().to_owned());
            }
            continue;
        }
        // A blank line ends one event.
        let payload = data.join("\n");
        data.clear();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        let event: Value =
            serde_json::from_str(&payload).map_err(|_| "ChatGPT sent an unreadable event")?;
        let kind = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match kind {
            "response.output_text.delta" => {
                let delta = event
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                out.text.push_str(delta);
                progress(delta);
            }
            "response.output_item.done" => {
                if let Some(item) = event.get("item") {
                    collect_item(&mut out.citations, item);
                }
            }
            "response.output_text.annotation.added" => {
                if event.pointer("/annotation/type").and_then(Value::as_str) == Some("url_citation")
                {
                    add_url(&mut out.citations, event.pointer("/annotation/url"));
                }
            }
            "response.failed" | "response.incomplete" | "error" => {
                let code = event
                    .pointer("/response/error/code")
                    .or_else(|| event.pointer("/error/code"))
                    .and_then(Value::as_str)
                    .unwrap_or(kind);
                return Err(format!("ChatGPT did not complete: {code}"));
            }
            "response.completed" => {
                if let Some(status) = event.pointer("/response/status").and_then(Value::as_str)
                    && status != "completed"
                {
                    return Err("Response completion status was invalid".into());
                }
                let output = event
                    .pointer("/response/output")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let mut completed_text = String::new();
                for item in &output {
                    collect_item(&mut out.citations, item);
                    for c in item
                        .get("content")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        if c.get("type").and_then(Value::as_str) == Some("output_text") {
                            completed_text.push_str(
                                c.get("text").and_then(Value::as_str).unwrap_or_default(),
                            );
                        }
                    }
                }
                // The completed response's text is canonical over the deltas.
                if !completed_text.is_empty() {
                    out.text = completed_text;
                }
                return Ok(out);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stream(events: &[Value]) -> String {
        events.iter().fold(String::new(), |mut s, e| {
            use std::fmt::Write as _;
            let _ = write!(s, "data: {e}\r\n\r\n");
            s
        })
    }

    fn run(events: &[Value]) -> Result<Completed, String> {
        read(stream(events).as_bytes(), |_| {})
    }

    #[test]
    fn requires_completion_even_when_partial_json_looks_valid() {
        let err = run(&[
            json!({"type": "response.output_text.delta", "delta": "{\"schema_version\":1}"}),
        ])
        .unwrap_err();
        assert!(err.contains("Interrupted"));
    }

    #[test]
    fn handles_crlf_and_records_source_urls() {
        let got = run(&[
            json!({"type": "response.output_text.delta", "delta": "{\"ok\":true}"}),
            json!({"type": "response.completed", "response": {"status": "completed",
                "output": [{"action": {"sources": [{"url": "https://example.com/track"}]}}]}}),
        ])
        .unwrap();
        assert_eq!(got.text, "{\"ok\":true}");
        assert!(got.citations.contains("https://example.com/track"));
    }

    #[test]
    fn rejects_failed_incomplete_and_error_events() {
        for kind in ["response.failed", "response.incomplete", "error"] {
            assert!(
                run(&[json!({"type": kind})])
                    .unwrap_err()
                    .contains("did not complete")
            );
        }
    }

    #[test]
    fn keeps_opened_pages_and_incremental_citations() {
        let got = run(&[
            json!({"type": "response.output_item.done", "item": {"type": "web_search_call",
                "action": {"type": "open_page", "url": "https://music.apple.com/us/song/1572859302"}}}),
            json!({"type": "response.output_item.done", "item": {"type": "web_search_call",
                "action": {"type": "find_in_page", "url": "https://example.com/credits"}}}),
            json!({"type": "response.output_text.annotation.added",
                "annotation": {"type": "url_citation", "url": "https://example.com/release"}}),
            json!({"type": "response.output_text.delta", "delta": "{\"ok\":true}"}),
            json!({"type": "response.completed", "response": {"status": "completed", "output": []}}),
        ])
        .unwrap();
        assert_eq!(
            got.citations.into_iter().collect::<Vec<_>>(),
            [
                "https://example.com/credits",
                "https://example.com/release",
                "https://music.apple.com/us/song/1572859302"
            ]
        );
    }

    #[test]
    fn takes_completed_text_and_rejects_unsafe_citations() {
        let got = run(&[
            json!({"type": "response.output_text.delta", "delta": "{\"partial\":"}),
            json!({"type": "response.completed", "response": {"status": "completed", "output": [
            {"content": [{"type": "output_text", "text": "{\"ok\":true}", "annotations": [
                {"type": "url_citation", "url": "https://example.com/facts"},
                {"type": "url_citation", "url": "javascript:alert(1)"},
                {"type": "url_citation", "url": "https://user:secret@example.com"},
                {"type": "file_citation", "url": "https://example.com/invented"}
            ]}]}]}}),
        ])
        .unwrap();
        assert_eq!(got.text, "{\"ok\":true}");
        assert_eq!(
            got.citations.into_iter().collect::<Vec<_>>(),
            ["https://example.com/facts"]
        );
    }

    #[test]
    fn an_incomplete_stream_returns_no_research() {
        let err = run(&[
            json!({"type": "response.output_item.done", "item": {"action": {"type": "open_page", "url": "https://example.com"}}}),
            json!({"type": "response.incomplete"}),
        ])
        .unwrap_err();
        assert!(err.contains("did not complete"));
    }

    #[test]
    fn progress_sees_each_delta() {
        let mut seen = String::new();
        let body = stream(&[
            json!({"type": "response.output_text.delta", "delta": "ab"}),
            json!({"type": "response.output_text.delta", "delta": "c"}),
            json!({"type": "response.completed", "response": {"status": "completed", "output": []}}),
        ]);
        let got = read(body.as_bytes(), |d| seen.push_str(d)).unwrap();
        assert_eq!((seen.as_str(), got.text.as_str()), ("abc", "abc"));
    }
}
