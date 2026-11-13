//! Sanskrit verses: the computed analysis (kosha_core::sanskrit: transliteration, meter, words in
//! the dictionaries) and the explanation written by Google Gemini from it (your key, Settings →
//! Online). Gemini gets the computed facts and is told to keep to them: it writes the prose (the
//! translation, the interpretation, the use), the facts stay Kosha's.

use std::sync::OnceLock;
use std::time::Duration;

use kosha_core::sanskrit::{Analysis, Analyzer};
use serde_json::{json, Value};
use tauri::State;

use crate::Shared;

fn analyzer() -> &'static Analyzer {
    static A: OnceLock<Analyzer> = OnceLock::new();
    A.get_or_init(Analyzer::new)
}

#[tauri::command]
pub fn sanskrit_analyze(state: State<Shared>, text: String) -> Result<Analysis, String> {
    if text.trim().is_empty() {
        return Err("Paste a verse first".into());
    }
    if text.chars().count() > 2000 {
        return Err("That is long for one verse: at most 2,000 characters".into());
    }
    let lib = state.library.read().unwrap();
    crate::guarded_result(|| Ok(analyzer().analyze(&text, lib.as_ref())))
}

/// What Gemini is given: the verse and everything Kosha computed about it.
fn prompt(a: &Analysis) -> String {
    let mut facts = String::new();
    for (d, i) in a.deva.iter().zip(&a.iast) {
        facts.push_str(&format!("{d}\n{i}\n"));
    }
    if let Some(m) = &a.meter {
        facts.push_str(&format!(
            "\nMeter (computed from the syllables): {} ({})\n",
            m.name, m.note
        ));
    }
    facts.push_str(
        "\nWords, split and looked up in Sanskrit dictionaries (Monier-Williams, Apte…):\n",
    );
    for w in &a.words {
        if w.parts.is_empty() {
            facts.push_str(&format!("- {}: not found in the dictionaries\n", w.word));
            continue;
        }
        let forms: Vec<String> = w.parts.iter().map(|p| p.iast.clone()).collect();
        facts.push_str(&format!("- {} = {}\n", w.word, forms.join(" + ")));
        for p in &w.parts {
            for m in p.meanings.iter().take(2) {
                let g: String = m.gloss.chars().take(160).collect();
                facts.push_str(&format!("    {} ({}): {g}\n", p.iast, m.dict_name));
            }
        }
    }
    format!(
        "You are a careful Sanskritist. Explain this Sanskrit verse for a learner.\n\n\
         VERSE AND COMPUTED FACTS\n{facts}\n\
         RULES\n\
         - The word splits and dictionary forms above were computed from dictionaries. Keep to them; \
           if one is clearly wrong, say so in \"notes\" instead of silently using another.\n\
         - Do not invent grammatical analyses. Give a case, tense or person only when you are sure.\n\
         - If you are not sure where the verse comes from, leave \"source\" empty.\n\
         - Write in English. Plain text in every field (no Markdown).\n\n\
         Answer with JSON only, in this form:\n\
         {{\"source\": \"text, chapter and verse, e.g. Bhagavad Gītā 2.2, or empty\",\n\
          \"literal\": [\"a literal translation of each line, in order\"],\n\
          \"translation\": \"a natural translation\",\n\
          \"words\": [{{\"word\": \"as in the verse (IAST)\", \"sense\": \"its sense here, briefly\"}}],\n\
          \"interpretation\": [{{\"school\": \"e.g. Advaita Vedānta (Śaṅkara)\", \"text\": \"how it is read\"}}],\n\
          \"application\": \"how the verse can be used in life or practice\",\n\
          \"notes\": \"anything uncertain, or computed facts that look wrong\"}}"
    )
}

/// Gemini's explanation of an analysed verse (a JSON object as in `prompt`).
#[tauri::command]
pub async fn sanskrit_explain(
    state: State<'_, Shared>,
    analysis: Analysis,
) -> Result<Value, String> {
    let o = state.settings.lock().unwrap().online.clone();
    let key = o.gemini_key.trim().to_string();
    if key.is_empty() {
        return Err("no-key".into());
    }
    let model = if o.gemini_model.trim().is_empty() {
        "gemini-flash-latest".to_string()
    } else {
        o.gemini_model.trim().to_string()
    };
    let body = json!({
        "contents": [{ "role": "user", "parts": [{ "text": prompt(&analysis) }] }],
        "generationConfig": { "temperature": 0.3, "responseMimeType": "application/json" }
    });
    tauri::async_runtime::spawn_blocking(move || {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(90))
            .build();
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
        );
        let resp = agent.post(&url).set("x-goog-api-key", &key).send_json(body);
        let v: Value = match resp {
            Ok(r) => r.into_json().map_err(|e| e.to_string())?,
            Err(ureq::Error::Status(code, r)) => {
                // Google says what is wrong in the body: show its message
                let msg = r
                    .into_json::<Value>()
                    .ok()
                    .and_then(|v| {
                        v.pointer("/error/message")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .unwrap_or_default();
                return Err(match code {
                    400 | 403 if msg.to_lowercase().contains("api key") => {
                        format!("Gemini refused the key: {msg}")
                    }
                    404 => format!(
                        "Gemini has no model “{model}”: change it in Settings → Online ({msg})"
                    ),
                    429 => {
                        format!("Gemini's free limit is reached for now: try again later ({msg})")
                    }
                    _ => format!("Gemini answered {code}: {msg}"),
                });
            }
            Err(_) => return Err("No connection to Gemini".into()),
        };
        let text = v
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                let why = v
                    .pointer("/promptFeedback/blockReason")
                    .and_then(Value::as_str)
                    .unwrap_or("no text");
                format!("Gemini gave no answer ({why})")
            })?;
        // the JSON may come wrapped in a code fence
        let t = text
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        serde_json::from_str::<Value>(t)
            .map_err(|_| "Gemini's answer was not in the expected form: try again".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
