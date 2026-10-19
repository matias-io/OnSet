//! Reading each CLI's event stream: the structured answer, and the HTTPS pages it actually
//! consulted (search results and opened pages), as each tool reports them.
use std::collections::{BTreeSet, HashMap};

use serde_json::Value;

#[derive(Debug)]
pub struct Parsed {
    pub value: Value,
    pub consulted: BTreeSet<String>,
}

fn add(consulted: &mut BTreeSet<String>, url: Option<&str>) {
    let Some(url) = url else { return };
    if let Ok(u) = tauri::Url::parse(url)
        && u.scheme() == "https"
        && u.username().is_empty()
        && u.password().is_none()
    {
        consulted.insert(url.to_owned());
    }
}

fn events(stdout: &str) -> impl Iterator<Item = Value> + '_ {
    stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
}

/// Every `"url":"https://…"` inside a tool result's text, as Claude's web search lists links.
fn urls_in_text(text: &str, consulted: &mut BTreeSet<String>) {
    let mut rest = text;
    while let Some(i) = rest.find("\"url\":\"") {
        rest = &rest[i + 7..];
        if let Some(end) = rest.find('"') {
            add(consulted, Some(&rest[..end]));
            rest = &rest[end..];
        }
    }
}

/// `claude -p --output-format stream-json --json-schema`: the final `result` event carries
/// `structured_output`. Search results list links; a fetch counts once it succeeded.
pub fn claude(stdout: &str) -> Result<Parsed, String> {
    let mut consulted = BTreeSet::new();
    let mut fetches: HashMap<String, String> = HashMap::new();
    for e in events(stdout) {
        match e.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                for c in e
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if c.get("type").and_then(Value::as_str) == Some("tool_use")
                        && c.get("name").and_then(Value::as_str) == Some("WebFetch")
                        && let (Some(id), Some(url)) = (
                            c.get("id").and_then(Value::as_str),
                            c.pointer("/input/url").and_then(Value::as_str),
                        )
                    {
                        fetches.insert(id.to_owned(), url.to_owned());
                    }
                }
            }
            Some("user") => {
                for c in e
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if c.get("type").and_then(Value::as_str) != Some("tool_result")
                        || c.get("is_error").and_then(Value::as_bool) == Some(true)
                    {
                        continue;
                    }
                    let id = c
                        .get("tool_use_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if let Some(url) = fetches.get(id) {
                        add(&mut consulted, Some(url));
                    } else {
                        let text = match c.get("content") {
                            Some(Value::String(s)) => s.clone(),
                            Some(other) => other.to_string(),
                            None => String::new(),
                        };
                        urls_in_text(&text, &mut consulted);
                    }
                }
            }
            Some("result") => {
                if e.get("is_error").and_then(Value::as_bool) == Some(true)
                    || e.get("subtype").and_then(Value::as_str) != Some("success")
                {
                    let why = e
                        .get("result")
                        .and_then(Value::as_str)
                        .unwrap_or("did not complete");
                    return Err(format!("Claude: {why}"));
                }
                let value = e
                    .get("structured_output")
                    .cloned()
                    .ok_or("Claude returned no structured answer")?;
                return Ok(Parsed { value, consulted });
            }
            _ => {}
        }
    }
    Err("Claude's answer was cut off".into())
}

/// `codex exec --json --output-schema`: the last agent message is the JSON answer; each
/// `web_search` item lists its results' URLs, and opened pages carry their own.
pub fn codex(stdout: &str) -> Result<Parsed, String> {
    let mut consulted = BTreeSet::new();
    let mut last_message: Option<String> = None;
    let mut completed = false;
    for e in events(stdout) {
        match e.get("type").and_then(Value::as_str) {
            Some("item.completed") => {
                let item = &e["item"];
                match item.get("type").and_then(Value::as_str) {
                    Some("agent_message") => {
                        last_message = item.get("text").and_then(Value::as_str).map(str::to_owned);
                    }
                    Some("web_search") => {
                        for r in item
                            .get("results")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                        {
                            add(&mut consulted, r.get("url").and_then(Value::as_str));
                        }
                        if matches!(
                            item.pointer("/action/type").and_then(Value::as_str),
                            Some("open_page" | "find_in_page")
                        ) {
                            add(
                                &mut consulted,
                                item.pointer("/action/url").and_then(Value::as_str),
                            );
                        }
                    }
                    _ => {}
                }
            }
            Some("turn.completed") => completed = true,
            Some("turn.failed" | "error") => {
                let why = e
                    .pointer("/error/message")
                    .or_else(|| e.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("did not complete");
                return Err(format!("Codex: {why}"));
            }
            _ => {}
        }
    }
    if !completed {
        return Err("Codex's answer was cut off".into());
    }
    let text = last_message.ok_or("Codex returned no answer")?;
    let value = serde_json::from_str(text.trim()).map_err(|_| "Codex returned invalid JSON")?;
    Ok(Parsed { value, consulted })
}

/// `agy -p --output-format stream-json --json-schema`: the final `result` event carries
/// `structured_output`; finished read-URL steps name the pages opened.
pub fn gemini(stdout: &str) -> Result<Parsed, String> {
    let mut consulted = BTreeSet::new();
    for e in events(stdout) {
        if let Some(step) = e.get("step_update")
            && step.get("state").and_then(Value::as_str) == Some("DONE")
        {
            add(
                &mut consulted,
                step.pointer("/tool_info/parameters/Url")
                    .and_then(Value::as_str),
            );
        }
        if e.get("event").and_then(Value::as_str) == Some("result") {
            let r = &e["result"];
            if r.get("status").and_then(Value::as_str) != Some("SUCCESS") {
                let why = r
                    .get("error")
                    .or_else(|| r.get("error_message"))
                    .or_else(|| r.get("status"))
                    .and_then(Value::as_str)
                    .unwrap_or("did not complete");
                return Err(format!("Gemini: {why}"));
            }
            let value = r
                .get("structured_output")
                .cloned()
                .ok_or("Gemini returned no structured answer")?;
            return Ok(Parsed { value, consulted });
        }
    }
    Err("Gemini's answer was cut off".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn lines(events: &[Value]) -> String {
        events.iter().fold(String::new(), |mut s, e| {
            use std::fmt::Write as _;
            let _ = writeln!(s, "{e}");
            s
        })
    }

    #[test]
    fn claude_answer_and_only_successful_fetches() {
        let out = lines(&[
            json!({"type": "assistant", "message": {"content": [
                {"type": "tool_use", "id": "a", "name": "WebSearch", "input": {"query": "q"}},
                {"type": "tool_use", "id": "b", "name": "WebFetch", "input": {"url": "https://en.wikipedia.org/wiki/X"}},
                {"type": "tool_use", "id": "c", "name": "WebFetch", "input": {"url": "https://denied.example/"}}]}}),
            json!({"type": "user", "message": {"content": [
                {"type": "tool_result", "tool_use_id": "a", "content": "Links: [{\"title\":\"T\",\"url\":\"https://musicbrainz.org/r/1\"},{\"url\":\"javascript:x\"}]"},
                {"type": "tool_result", "tool_use_id": "b", "content": "The infobox lists"},
                {"type": "tool_result", "tool_use_id": "c", "content": "not granted", "is_error": true}]}}),
            json!({"type": "result", "subtype": "success", "is_error": false, "structured_output": {"genre": "Trance"}}),
        ]);
        let p = claude(&out).unwrap();
        assert_eq!(p.value, json!({"genre": "Trance"}));
        assert_eq!(
            p.consulted.into_iter().collect::<Vec<_>>(),
            [
                "https://en.wikipedia.org/wiki/X",
                "https://musicbrainz.org/r/1"
            ]
        );
    }

    #[test]
    fn claude_errors_and_cut_off_streams_fail() {
        assert!(
            claude(&lines(&[
                json!({"type": "result", "subtype": "error_max_turns", "is_error": true})
            ]))
            .is_err()
        );
        assert!(
            claude(&lines(&[json!({"type": "system"})]))
                .unwrap_err()
                .contains("cut off")
        );
    }

    #[test]
    fn codex_answer_and_search_results() {
        let out = lines(&[
            json!({"type": "thread.started"}),
            json!({"type": "item.completed", "item": {"type": "agent_message", "text": "I'll check online."}}),
            json!({"type": "item.completed", "item": {"type": "web_search", "results": [
                {"url": "https://musicbrainz.org/release/1"}, {"url": "http://insecure.example"}]}}),
            json!({"type": "item.completed", "item": {"type": "agent_message", "text": "{\"genre\":\"Trance\"}"}}),
            json!({"type": "turn.completed"}),
        ]);
        let p = codex(&out).unwrap();
        assert_eq!(p.value, json!({"genre": "Trance"}));
        assert_eq!(
            p.consulted.into_iter().collect::<Vec<_>>(),
            ["https://musicbrainz.org/release/1"]
        );
    }

    #[test]
    fn codex_failures_and_unfinished_turns_fail() {
        assert!(
            codex(&lines(&[
                json!({"type": "turn.failed", "error": {"message": "usage limit"}})
            ]))
            .unwrap_err()
            .contains("usage limit")
        );
        assert!(
            codex(&lines(&[
                json!({"type": "item.completed", "item": {"type": "agent_message", "text": "{}"}})
            ]))
            .is_err()
        );
    }

    #[test]
    fn gemini_answer_and_opened_pages() {
        let out = lines(&[
            json!({"step_update": {"state": "DONE", "tool_info": {"parameters": {"Url": "https://en.wikipedia.org/wiki/X"}}}}),
            json!({"step_update": {"state": "RUNNING", "tool_info": {"parameters": {"Url": "https://pending.example"}}}}),
            json!({"event": "result", "result": {"status": "SUCCESS", "structured_output": {"genre": "Trance"}}}),
        ]);
        let p = gemini(&out).unwrap();
        assert_eq!(p.value, json!({"genre": "Trance"}));
        assert_eq!(
            p.consulted.into_iter().collect::<Vec<_>>(),
            ["https://en.wikipedia.org/wiki/X"]
        );
        assert!(
            gemini(&lines(&[
                json!({"event": "result", "result": {"status": "ERROR"}})
            ]))
            .is_err()
        );
    }
}
