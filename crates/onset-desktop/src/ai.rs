//! The AI features built on the providers: track details lookups, comparing the planner's
//! set with each AI's pick, and structure reads from the spectrogram or the audio.
//!
//! Each feature asks any chosen providers at once and keeps every answer separate, labelled
//! with its provider, so they can be compared rather than blended. Nothing here saves on its
//! own: the caller sends results through the service, which validates them.
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::chatgpt::{self, ChatGpt};
use crate::providers::{self, Answer, Ask, Provider};

/// Asks one provider: a CLI, or ChatGPT with the account's selected model.
pub fn ask(provider: Provider, ask: &Ask, chat: &ChatGpt) -> Result<Answer, String> {
    if provider != Provider::ChatGpt {
        return providers::ask_cli(provider, ask);
    }
    if !ask.images.is_empty() {
        return Err("ChatGPT reads here take text only".into());
    }
    let model = ask
        .model
        .clone()
        .or_else(|| chat.status()["selected_model"].as_str().map(str::to_owned))
        .ok_or("Choose a ChatGPT model first")?;
    let started = Instant::now();
    let (value, consulted) = chat.ask_json(
        &ask.instructions,
        &providers::prompt(ask, &[]),
        &ask.schema,
        &model,
        ask.web,
        |_| {},
    )?;
    Ok(Answer {
        provider,
        value,
        consulted,
        seconds: started.elapsed().as_secs_f64(),
    })
}

/// Asks every provider in parallel; each result stands alone.
pub fn ask_all(
    chosen: &[Provider],
    request: &(dyn Fn(Provider) -> Ask + Sync),
    chat: &ChatGpt,
) -> Vec<(Provider, Result<Answer, String>)> {
    std::thread::scope(|scope| {
        let running: Vec<_> = chosen
            .iter()
            .map(|&p| (p, scope.spawn(move || ask(p, &request(p), chat))))
            .collect();
        running
            .into_iter()
            .map(|(p, h)| {
                (
                    p,
                    h.join()
                        .unwrap_or_else(|_| Err(format!("{} stopped unexpectedly", p.name()))),
                )
            })
            .collect()
    })
}

// ---------------------------------------------------------------------------------------------
// Track details: genre, mood, instrumentation, release, description.

pub fn details_ask(context: &Value, web: bool) -> Ask {
    let mut evidence = context.clone();
    evidence["task"] = "enrichment".into();
    Ask {
        instructions: format!(
            "{}\nThis request: enrichment for every supplied track.",
            chatgpt::INSTRUCTIONS
        ),
        evidence,
        schema: chatgpt::schema(true),
        web,
        images: Vec::new(),
        model: None,
        timeout: Duration::from_secs(600),
    }
}

/// One enrichment record per track from all providers' answers. Every claim keeps which
/// provider made it and passes the same citation check: a web claim whose page that provider
/// did not consult becomes an unverified model interpretation.
pub fn merge_details(answers: &[Answer], web: bool) -> Value {
    let mut by_track: BTreeMap<String, Value> = BTreeMap::new();
    for a in answers {
        let mut value = a.value.clone();
        chatgpt::citations::retain_enrichment_evidence(
            &mut value,
            &a.consulted,
            a.provider.name(),
            web,
        );
        for record in value["records"].as_array().into_iter().flatten() {
            let Some(id) = record["track_id"].as_str() else {
                continue;
            };
            let entry = by_track.entry(id.to_owned()).or_insert_with(|| {
                json!({"track_id": id, "claims": [], "research": {
                    "requested": web, "completed": true, "providers": [], "consulted_sources": []}})
            });
            for claim in record["claims"].as_array().into_iter().flatten() {
                let mut claim = claim.clone();
                claim["provider"] = a.provider.name().into();
                entry["claims"]
                    .as_array_mut()
                    .expect("claims array")
                    .push(claim);
            }
            entry["research"]["providers"]
                .as_array_mut()
                .expect("providers array")
                .push(json!({"provider": a.provider.name(), "consulted_sources": a.consulted}));
        }
    }
    for entry in by_track.values_mut() {
        let union: BTreeSet<String> = entry["research"]["providers"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|p| {
                p["consulted_sources"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .filter_map(|u| u.as_str().map(str::to_owned))
            .collect();
        entry["research"]["consulted_sources"] = json!(union);
    }
    json!({"schema_version": 1, "records": by_track.into_values().collect::<Vec<_>>()})
}

// ---------------------------------------------------------------------------------------------
// Sets: the planner's choice beside each AI's.

/// The tracks worth sending: those in the planner's sets, then must-plays, at most `limit`.
/// A shortlist keeps the request small enough for a considered answer.
pub fn shortlist(plans: &[Value], brief: &Value, limit: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let ids = plans
        .iter()
        .flat_map(|p| p["track_ids"].as_array().cloned().unwrap_or_default())
        .chain(brief["must_play"].as_array().cloned().unwrap_or_default());
    for id in ids.filter_map(|v| v.as_str().map(str::to_owned)) {
        if !out.contains(&id) && out.len() < limit {
            out.push(id);
        }
    }
    out
}

pub fn set_schema() -> Value {
    json!({"type": "object", "additionalProperties": false,
        "required": ["schema_version", "track_ids", "transition_ids", "explanation", "critique", "prefers"],
        "properties": {
            "schema_version": {"type": "integer", "enum": [1]},
            "track_ids": {"type": "array", "items": {"type": "string"}},
            "transition_ids": {"type": "array", "items": {"type": "string"}},
            "explanation": {"type": "string"},
            "critique": {"type": "string"},
            "prefers": {"type": "string", "enum": ["own", "planner"]}}})
}

pub fn set_ask(context: &Value, planner_choice: &Value) -> Ask {
    Ask {
        instructions: "You are a DJ preparation adviser comparing notes with a set-planning \
            algorithm. Choose a set from the supplied tracks: track_ids in playing order, and for \
            each adjacent pair one transition_id from the supplied transitions whose track_ids \
            match that pair (validated_routes are complete ready-made options). Use only supplied \
            IDs and respect the brief. Then critique the planner's choice in planner_choice: what \
            works, what you would change and why, in plain DJ terms. Say whether you prefer your \
            set or the planner's."
            .into(),
        evidence: json!({"context": context, "planner_choice": planner_choice}),
        schema: set_schema(),
        web: false,
        images: Vec::new(),
        model: None,
        timeout: Duration::from_secs(600),
    }
}

/// How two orders of tracks compare: shared tracks, the same opening, and the share of
/// shared pairs played in the same relative order.
pub fn agreement(planner: &[String], ai: &[String]) -> Value {
    let shared: Vec<&String> = planner.iter().filter(|t| ai.contains(t)).collect();
    let pos = |list: &[String], t: &String| list.iter().position(|x| x == t).unwrap_or(0);
    let (mut same, mut pairs) = (0u32, 0u32);
    for i in 0..shared.len() {
        for j in i + 1..shared.len() {
            pairs += 1;
            let planner_order = pos(planner, shared[i]) < pos(planner, shared[j]);
            if planner_order == (pos(ai, shared[i]) < pos(ai, shared[j])) {
                same += 1;
            }
        }
    }
    json!({
        "shared_tracks": shared.len(),
        "planner_only": planner.len() - shared.len(),
        "ai_only": ai.iter().filter(|t| !planner.contains(t)).count(),
        "same_opening": !planner.is_empty() && planner.first() == ai.first(),
        "order_agreement": if pairs == 0 { Value::Null } else { json!(f64::from(same) / f64::from(pairs)) },
    })
}

// ---------------------------------------------------------------------------------------------
// Structure reads from the spectrogram, or from the audio for Gemini.

const EVENT_NAMES: [&str; 4] = ["drop", "breakdown", "second_drop", "outro"];

pub fn structure_schema() -> Value {
    let event = json!({"type": "object", "additionalProperties": false, "required": ["state", "bar"],
        "properties": {"state": {"type": "string", "enum": ["present", "absent", "unknown"]},
                       "bar": {"type": ["integer", "null"]}}});
    let mut properties = serde_json::Map::new();
    for name in EVENT_NAMES {
        properties.insert(name.into(), event.clone());
    }
    properties.insert(
        "confidence".into(),
        json!({"type": "string", "enum": ["low", "medium", "high"]}),
    );
    properties.insert("notes".into(), json!({"type": "string"}));
    let mut required: Vec<&str> = EVENT_NAMES.to_vec();
    required.extend(["confidence", "notes"]);
    json!({"type": "object", "additionalProperties": false, "required": required, "properties": properties})
}

/// `image` is the service's spectrogram-image result; with `audio`, the audio file is read
/// instead of the image.
pub fn structure_ask(image: &Value, attachment: std::path::PathBuf, audio: bool) -> Ask {
    let how = if audio {
        "Listen to the attached audio file. Bars are counted from the first downbeat as bar 0; \
         use the bar times in the evidence to convert what you hear into bar numbers."
    } else {
        "Look at the attached spectrogram image. It is drawn so bars can be counted exactly; \
         the evidence says how to read it."
    };
    Ask {
        instructions: format!(
            "You are reading the structure of an electronic dance track for a DJ. {how} Report, as \
             bar numbers: drop = the first sustained full groove (the first drop); breakdown = the \
             first breakdown after it; second_drop = the next drop after that breakdown; outro = \
             where the outro starts. Use state present with a bar when you can place it, absent \
             when the track clearly has no such part, and unknown when you cannot tell. Do not \
             guess beyond what you can see or hear. Say how sure you are and note anything \
             unusual (fills, fake drops, a missing intro)."
        ),
        evidence: json!({
            "title": image["title"], "artist": image["artist"], "bars": image["bars"],
            "duration_seconds": image["duration"], "how_to_read": image["how_to_read"],
            "bar_times_seconds": image["bar_times"],
        }),
        schema: structure_schema(),
        web: false,
        images: vec![attachment],
        model: None,
        timeout: Duration::from_secs(600),
    }
}

/// The observations-import request for one provider's read.
pub fn structure_import(answer: &Answer, image: &Value, audio: bool) -> Value {
    let medium = if audio { "audio" } else { "spectrogram" };
    let events: serde_json::Map<String, Value> = EVENT_NAMES
        .iter()
        .map(|e| ((*e).to_owned(), answer.value[*e].clone()))
        .collect();
    json!({
        "track_id": image["track_id"],
        "revision": image["revision"],
        "source": format!("{}:{medium}", answer.provider.name().to_lowercase()),
        "family": if audio { "audio-llm" } else { "spectrogram-llm" },
        "events": events,
        // Provider reads stay unverified until listening confirms them; their own estimate
        // of certainty is kept in the notes.
        "confidence": "unverified",
        "notes": format!("{} says {} confidence. {}", answer.provider.name(),
            answer.value["confidence"].as_str().unwrap_or("unstated"),
            answer.value["notes"].as_str().unwrap_or("")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(provider: Provider, value: Value, consulted: &[&str]) -> Answer {
        Answer {
            provider,
            value,
            consulted: consulted.iter().map(|s| (*s).to_owned()).collect(),
            seconds: 1.0,
        }
    }

    #[test]
    fn details_from_two_providers_merge_per_track_with_provenance() {
        let claude = answer(
            Provider::Claude,
            json!({"schema_version": 1, "records": [{"track_id": "t1", "claims": [
                {"kind": "genre", "value": "Tech house", "source_type": "web",
                 "url": "https://example.com/a", "confidence": "high"}]}]}),
            &["https://example.com/a"],
        );
        let codex = answer(
            Provider::Codex,
            json!({"schema_version": 1, "records": [{"track_id": "t1", "claims": [
                {"kind": "genre", "value": "House", "source_type": "web",
                 "url": "https://invented.example", "confidence": "high"}]}]}),
            &["https://example.com/b"],
        );
        let merged = merge_details(&[claude, codex], true);
        let records = merged["records"].as_array().unwrap();
        assert_eq!(records.len(), 1);
        let claims = records[0]["claims"].as_array().unwrap();
        assert_eq!(claims[0]["provider"], "Claude");
        assert_eq!(claims[0]["citation_check"]["status"], "retained");
        assert_eq!(claims[1]["provider"], "Codex");
        assert_eq!(claims[1]["source_type"], "model");
        assert_eq!(claims[1]["confidence"], "unverified");
        assert_eq!(
            records[0]["research"]["consulted_sources"],
            json!(["https://example.com/a", "https://example.com/b"])
        );
    }

    #[test]
    fn shortlist_keeps_plan_order_adds_must_plays_and_caps() {
        let plans = vec![
            json!({"track_ids": ["a", "b", "c"]}),
            json!({"track_ids": ["b", "d"]}),
        ];
        let brief = json!({"must_play": ["z"]});
        assert_eq!(shortlist(&plans, &brief, 10), ["a", "b", "c", "d", "z"]);
        assert_eq!(shortlist(&plans, &brief, 2), ["a", "b"]);
    }

    #[test]
    fn agreement_counts_shared_tracks_and_order() {
        let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
        let a = agreement(&s(&["a", "b", "c", "d"]), &s(&["a", "c", "b", "e"]));
        assert_eq!(a["shared_tracks"], 3);
        assert_eq!(a["planner_only"], 1);
        assert_eq!(a["ai_only"], 1);
        assert_eq!(a["same_opening"], true);
        // Pairs (a,b) (a,c) agree, (b,c) is swapped: 2 of 3.
        assert!((a["order_agreement"].as_f64().unwrap() - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(
            agreement(&s(&["a"]), &s(&["b"]))["order_agreement"],
            Value::Null
        );
    }

    #[test]
    fn structure_schema_is_strict_and_reads_import_as_their_own_source() {
        let schema = structure_schema();
        assert_eq!(schema["required"].as_array().unwrap().len(), 6);
        assert_eq!(schema["additionalProperties"], false);
        let read = answer(
            Provider::Gemini,
            json!({"drop": {"state": "present", "bar": 16}, "breakdown": {"state": "unknown", "bar": null},
                   "second_drop": {"state": "absent", "bar": null}, "outro": {"state": "present", "bar": 80},
                   "confidence": "medium", "notes": "fake drop at bar 12"}),
            &[],
        );
        let image = json!({"track_id": "t1", "revision": "r1"});
        let req = structure_import(&read, &image, true);
        assert_eq!(req["source"], "gemini:audio");
        assert_eq!(req["family"], "audio-llm");
        assert_eq!(req["events"]["drop"]["bar"], 16);
        assert_eq!(req["confidence"], "unverified");
        assert!(
            req["notes"]
                .as_str()
                .unwrap()
                .contains("medium confidence. fake drop")
        );
        assert_eq!(
            structure_import(&read, &image, false)["source"],
            "gemini:spectrogram"
        );
    }
}
