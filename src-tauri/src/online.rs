//! Online sources after the dictionaries: Wikipedia (article summary), Google Translate (the
//! Cloud Translation API, your key) and DeepL (your API key). Each is switched on or off in
//! Settings; what you look up is sent to that service. Requests run on a worker thread.
//!
//! Direction of a translation: to your language, unless the text already is in it (as the
//! service detects): then to the language you learn.

use std::time::Duration;

use kosha_core::library::{Online, Settings};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::Shared;

const AGENT: &str = "Kosha/0.1 (dictionary app; https://github.com/shatyrko/kosha)";

#[derive(Debug, Clone, Serialize, Default)]
pub struct Found {
    pub source: String,
    /// the main result: a translation, or a summary
    pub text: String,
    /// Wikipedia: the article's title and short description
    pub title: String,
    pub description: String,
    pub from: String,
    pub to: String,
    /// where to read more
    pub url: String,
    pub image: String,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .user_agent(AGENT)
        .build()
}

fn enc(s: &str) -> String {
    utf8_percent_encode(s, NON_ALPHANUMERIC).to_string()
}

fn err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(400, _) => "The request was refused (400): is the key right?".into(),
        ureq::Error::Status(403, _) => "The key was refused (403)".into(),
        ureq::Error::Status(456, _) => "The DeepL quota for this month is used up".into(),
        ureq::Error::Status(429, _) => "Too many requests: try again in a moment".into(),
        ureq::Error::Status(code, _) => format!("The service answered {code}"),
        ureq::Error::Transport(_) => "No connection".into(),
    }
}

fn has_cyrillic(s: &str) -> bool {
    s.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c))
}

// ── Wikipedia ────────────────────────────────────────────────────────────

fn wikipedia(text: &str) -> Result<Option<Found>, String> {
    let lang = if has_cyrillic(text) { "ru" } else { "en" };
    let a = agent();
    let summary = |title: &str| -> Result<Option<Value>, String> {
        let url = format!(
            "https://{lang}.wikipedia.org/api/rest_v1/page/summary/{}?redirect=true",
            enc(&title.replace(' ', "_"))
        );
        match a.get(&url).call() {
            Ok(r) => r.into_json::<Value>().map(Some).map_err(|e| e.to_string()),
            Err(ureq::Error::Status(404, _)) => Ok(None),
            Err(e) => Err(err(e)),
        }
    };
    let mut page = summary(text)?;
    if page.is_none() {
        // not an exact title: the first search result
        let url = format!("https://{lang}.wikipedia.org/w/api.php?action=opensearch&format=json&limit=1&search={}", enc(text));
        let found: Value = a
            .get(&url)
            .call()
            .map_err(err)?
            .into_json()
            .map_err(|e| e.to_string())?;
        if let Some(t) = found.get(1).and_then(|l| l.get(0)).and_then(Value::as_str) {
            page = summary(t)?;
        }
    }
    let Some(p) = page else { return Ok(None) };
    if p.get("type").and_then(Value::as_str) == Some("disambiguation") && p.get("extract").is_none()
    {
        return Ok(None);
    }
    let s = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Ok(Some(Found {
        source: "wikipedia".into(),
        title: s("title"),
        description: s("description"),
        text: s("extract"),
        from: lang.into(),
        url: p
            .pointer("/content_urls/desktop/page")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        image: p
            .pointer("/thumbnail/source")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        ..Default::default()
    }))
}

// ── Google Translate ─────────────────────────────────────────────────────
// The official Cloud Translation API (v2, "Basic") with your key; without a key Kosha only offers
// to open translate.google.com in the browser (the page does that).

fn google_once(text: &str, to: &str, key: &str) -> Result<Found, String> {
    let body = serde_json::json!({ "q": text, "target": to, "format": "text" });
    let v: Value = agent()
        .post(&format!(
            "https://translation.googleapis.com/language/translate/v2?key={}",
            enc(key)
        ))
        .send_json(body)
        .map_err(err)?
        .into_json()
        .map_err(|e| e.to_string())?;
    let t = v
        .pointer("/data/translations/0")
        .ok_or("Unexpected answer from Google")?;
    Ok(Found {
        source: "google".into(),
        text: t
            .get("translatedText")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        from: t
            .get("detectedSourceLanguage")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase(),
        to: to.into(),
        url: format!(
            "https://translate.google.com/?sl=auto&tl={}&text={}&op=translate",
            enc(to),
            enc(text)
        ),
        ..Default::default()
    })
}

fn google(text: &str, o: &Online) -> Result<Option<Found>, String> {
    let key = o.google_key.trim();
    if key.is_empty() {
        return Ok(None);
    }
    let mut f = google_once(text, &o.native, key)?;
    if f.from.eq_ignore_ascii_case(&o.native) {
        f = google_once(text, &o.learning, key)?;
    }
    Ok((!f.text.trim().is_empty()).then_some(f))
}

// ── DeepL ────────────────────────────────────────────────────────────────

fn deepl_target(lang: &str) -> String {
    match lang.to_ascii_lowercase().as_str() {
        "en" => "EN-US".into(),
        "pt" => "PT-PT".into(),
        "zh" => "ZH-HANS".into(),
        other => other.to_ascii_uppercase(),
    }
}

fn deepl_once(text: &str, to: &str, key: &str) -> Result<Found, String> {
    let host = if key.ends_with(":fx") {
        "api-free.deepl.com"
    } else {
        "api.deepl.com"
    };
    let body = serde_json::json!({ "text": [text], "target_lang": deepl_target(to) });
    let v: Value = agent()
        .post(&format!("https://{host}/v2/translate"))
        .set("Authorization", &format!("DeepL-Auth-Key {key}"))
        .send_json(body)
        .map_err(err)?
        .into_json()
        .map_err(|e| e.to_string())?;
    let t = v
        .pointer("/translations/0")
        .ok_or("Unexpected answer from DeepL")?;
    Ok(Found {
        source: "deepl".into(),
        text: t
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        from: t
            .get("detected_source_language")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase(),
        to: to.into(),
        url: format!("https://www.deepl.com/translator#auto/{}/{}", to, enc(text)),
        ..Default::default()
    })
}

fn deepl(text: &str, o: &Online) -> Result<Option<Found>, String> {
    let key = o.deepl_key.trim();
    if key.is_empty() {
        return Ok(None);
    }
    let mut f = deepl_once(text, &o.native, key)?;
    if f.from.eq_ignore_ascii_case(&o.native) {
        f = deepl_once(text, &o.learning, key)?;
    }
    Ok((!f.text.trim().is_empty()).then_some(f))
}

// ── commands ─────────────────────────────────────────────────────────────

/// One source for one text; `None`: nothing found there.
#[tauri::command]
pub async fn online_lookup(
    state: State<'_, Shared>,
    source: String,
    text: String,
) -> Result<Option<Found>, String> {
    let o = state.settings.lock().unwrap().online.clone();
    let text = text.trim().to_string();
    if text.is_empty() || text.chars().count() > 1000 {
        return Ok(None);
    }
    tauri::async_runtime::spawn_blocking(move || match source.as_str() {
        "wikipedia" if o.wikipedia => wikipedia(&text),
        "google" if o.google => google(&text, &o),
        "deepl" if o.deepl => deepl(&text, &o),
        _ => Ok(None),
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn online_settings(state: State<Shared>) -> Online {
    state.settings.lock().unwrap().online.clone()
}

#[tauri::command]
pub fn set_online_settings(state: State<Shared>, online: Online) -> Result<(), String> {
    let mut s: std::sync::MutexGuard<Settings> = state.settings.lock().unwrap();
    s.online = online;
    s.save()
        .map_err(|e| format!("Can't save the settings: {e}"))
}
