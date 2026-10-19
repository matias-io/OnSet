//! Keeping only citations the completed response actually consulted. A web claim whose URL
//! was not among the research sources becomes an unverified model interpretation, and every
//! claim records why, so provenance is never lost.
use std::collections::BTreeSet;

use serde_json::{Value, json};

/// The exact URL, or for Apple Music song links the documented resource identity
/// (storefront and song id), so an optional slug does not break the match.
fn citation_identity(value: &str) -> Option<String> {
    let mut url = tauri::Url::parse(value).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let path = url.path().trim_end_matches('/').to_owned();
    if url.host_str() == Some("music.apple.com") && url.port().is_none() && url.query().is_none() {
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let song = match parts.as_slice() {
            [store, "song", id] | [store, "song", _, id] => Some((*store, *id)),
            _ => None,
        };
        if let Some((store, id)) = song
            && store.len() == 2
            && store.bytes().all(|b| b.is_ascii_lowercase())
            && !id.is_empty()
            && id.bytes().all(|b| b.is_ascii_digit())
        {
            return Some(format!("apple-song:{store}:{id}"));
        }
    }
    url.set_fragment(None);
    Some(url.to_string())
}

/// The consulted URL that supports `requested`, or `None`.
pub fn verified_citation(requested: Option<&str>, citations: &BTreeSet<String>) -> Option<String> {
    let requested = requested?;
    let identity = citation_identity(requested)?;
    if citations.contains(requested) {
        return Some(requested.to_owned());
    }
    citations
        .iter()
        .find(|c| citation_identity(c).as_deref() == Some(identity.as_str()))
        .cloned()
}

/// Annotates every enrichment claim with a citation check and downgrades unsupported web
/// claims, as the Electron shell did before handing records to the service.
pub fn retain_enrichment_evidence(
    result: &mut Value,
    citations: &BTreeSet<String>,
    model: &str,
    research: bool,
) {
    let Some(records) = result.get_mut("records").and_then(Value::as_array_mut) else {
        return;
    };
    for record in records {
        record["research"] = json!({
            "requested": research,
            "model": model,
            "completed": true,
            "consulted_sources": citations.iter().collect::<Vec<_>>(),
        });
        let Some(claims) = record.get_mut("claims").and_then(Value::as_array_mut) else {
            continue;
        };
        for claim in claims {
            let source_type = claim
                .get("source_type")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let url = claim.get("url").cloned().unwrap_or(Value::Null);
            let web = source_type.as_deref() == Some("web");
            let source = if web {
                verified_citation(url.as_str(), citations)
            } else {
                None
            };
            let (status, reason) = match (&source, web) {
                (Some(_), _) => (
                    "retained",
                    "URL matches completed response research evidence",
                ),
                (None, true) if citations.is_empty() => (
                    "unverified",
                    "Completed response supplied no research source URLs",
                ),
                (None, true) => (
                    "unverified",
                    "Claim URL was not among the response research sources",
                ),
                (None, false) => (
                    "not_requested",
                    "Model interpretation; no sourced fact asserted",
                ),
            };
            claim["citation_check"] = json!({
                "requested_source_type": source_type,
                "requested_url": url,
                "status": status,
                "reason": reason,
            });
            if let Some(s) = source {
                claim["url"] = s.into();
            } else if web {
                claim["source_type"] = "model".into();
                claim["url"] = Value::Null;
                claim["confidence"] = "unverified".into();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(urls: &[&str]) -> BTreeSet<String> {
        urls.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn apple_song_slug_differences_still_match() {
        let actual = "https://music.apple.com/us/song/1572859302";
        assert_eq!(
            verified_citation(
                Some("https://music.apple.com/us/song/fragments/1572859302"),
                &set(&[actual])
            )
            .as_deref(),
            Some(actual)
        );
    }

    #[test]
    fn different_song_store_host_type_or_query_do_not_match() {
        let cited = set(&["https://music.apple.com/us/song/1572859302"]);
        for v in [
            "https://music.apple.com/us/song/111",
            "https://music.apple.com/ca/song/1572859302",
            "https://evil.example/us/song/1572859302",
            "https://music.apple.com/us/album/1572859302",
            "https://music.apple.com/us/song/1572859302?i=111",
        ] {
            assert_eq!(verified_citation(Some(v), &cited), None, "{v}");
        }
    }

    #[test]
    fn unconsulted_and_credentialed_links_are_rejected() {
        assert_eq!(
            verified_citation(Some("https://example.com/claim"), &set(&[])),
            None
        );
        assert_eq!(
            verified_citation(
                Some("https://user:secret@example.com/claim"),
                &set(&["https://user:secret@example.com/claim"])
            ),
            None
        );
        assert_eq!(
            verified_citation(
                Some("https://example.com/claim"),
                &set(&["https://example.com/claim"])
            )
            .as_deref(),
            Some("https://example.com/claim")
        );
    }

    #[test]
    fn claims_are_retained_or_downgraded_with_their_provenance() {
        let actual = "https://music.apple.com/us/song/1572859302";
        let mut result = json!({"records": [{"track_id": "a", "claims": [
            {"source_type": "web", "url": "https://music.apple.com/us/song/fragments/1572859302", "confidence": "medium"},
            {"source_type": "web", "url": "https://example.com/not-consulted", "confidence": "high"},
            {"source_type": "model", "url": null, "confidence": "low"}
        ]}]});
        retain_enrichment_evidence(&mut result, &set(&[actual]), "selected-model", true);
        let r = &result["records"][0];
        assert_eq!(r["research"]["consulted_sources"], json!([actual]));
        assert_eq!(r["claims"][0]["url"], actual);
        assert_eq!(r["claims"][0]["citation_check"]["status"], "retained");
        assert_eq!(r["claims"][1]["source_type"], "model");
        assert_eq!(r["claims"][1]["url"], Value::Null);
        assert_eq!(r["claims"][1]["confidence"], "unverified");
        assert_eq!(
            r["claims"][1]["citation_check"]["requested_url"],
            "https://example.com/not-consulted"
        );
        assert_eq!(r["claims"][2]["citation_check"]["status"], "not_requested");
    }
}
