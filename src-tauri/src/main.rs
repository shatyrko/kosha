//! Kosha's window: the engine (kosha-core) behind a few commands for the page in `ui/`, and the
//! `kosha://` address that serves a dictionary's sounds and pictures.
//!
//! `kosha-app WORD` opens the window on that word; when Kosha is already running, the word goes to
//! the running window instead (single instance), so a desktop shortcut can look words up.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cards;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use kosha_core::library::{Failed, Hit, Library, Settings};
use kosha_core::{Article, DictInfo};
use percent_encoding::{percent_decode_str, utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Serialize;
use tauri::http::{header, Response, StatusCode};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
pub(crate) struct AppState {
    pub(crate) library: RwLock<Option<Library>>,
    settings: Mutex<Settings>,
    /// the word Kosha was started with, taken once by the page
    first_word: Mutex<Option<String>>,
    /// increases with every reload, so an older reload finishing late is ignored
    generation: Mutex<u64>,
}

pub(crate) type Shared = Arc<AppState>;

#[derive(Serialize)]
struct DictRow {
    info: DictInfo,
    enabled: bool,
}

#[derive(Serialize)]
struct LibraryView {
    loading: bool,
    dicts: Vec<DictRow>,
    folders: Vec<String>,
    failed: Vec<Failed>,
}

pub(crate) fn resource_url(dict: &str, name: &str) -> String {
    let name = utf8_percent_encode(name, NON_ALPHANUMERIC);
    if cfg!(windows) {
        format!("http://kosha.localhost/{dict}/{name}")
    } else {
        format!("kosha://localhost/{dict}/{name}")
    }
}

/// Open the dictionaries of the settings in the background; the page hears "library" when done.
fn reload(app: &AppHandle, state: &Shared) {
    let settings = state.settings.lock().unwrap().clone();
    let generation = {
        let mut g = state.generation.lock().unwrap();
        *g += 1;
        *g
    };
    *state.library.write().unwrap() = None;
    let _ = app.emit("library", false);
    let (app, state) = (app.clone(), state.clone());
    std::thread::spawn(move || {
        let lib = Library::from_settings(&settings);
        if *state.generation.lock().unwrap() == generation {
            *state.library.write().unwrap() = Some(lib);
            let _ = app.emit("library", true);
        }
    });
}

#[tauri::command]
fn search(state: State<Shared>, query: String, limit: Option<usize>) -> Vec<Hit> {
    match state.library.read().unwrap().as_ref() {
        Some(lib) => lib.search(&query, limit.unwrap_or(60)),
        None => Vec::new(),
    }
}

#[tauri::command]
fn lookup(state: State<Shared>, word: String) -> Vec<Article> {
    match state.library.read().unwrap().as_ref() {
        Some(lib) => lib.lookup(&word, &resource_url),
        None => Vec::new(),
    }
}

#[tauri::command]
fn library(state: State<Shared>) -> LibraryView {
    let settings = state.settings.lock().unwrap().clone();
    let lib = state.library.read().unwrap();
    LibraryView {
        loading: lib.is_none(),
        dicts: lib
            .as_ref()
            .map(|l| {
                l.infos()
                    .into_iter()
                    .map(|(info, enabled)| DictRow { info, enabled })
                    .collect()
            })
            .unwrap_or_default(),
        folders: settings
            .folders
            .iter()
            .map(|f| f.to_string_lossy().into_owned())
            .collect(),
        failed: lib.as_ref().map(|l| l.failed.clone()).unwrap_or_default(),
    }
}

#[derive(Serialize)]
struct About {
    version: &'static str,
    settings: String,
    cards: String,
    cache: String,
}

#[tauri::command]
fn about() -> About {
    About {
        version: env!("CARGO_PKG_VERSION"),
        settings: Settings::path().to_string_lossy().into_owned(),
        cards: kosha_core::cards::default_path()
            .to_string_lossy()
            .into_owned(),
        cache: kosha_core::library::cache_dir()
            .to_string_lossy()
            .into_owned(),
    }
}

#[tauri::command]
fn first_word(state: State<Shared>) -> Option<String> {
    state.first_word.lock().unwrap().take()
}

fn save(settings: &Settings) -> Result<(), String> {
    settings
        .save()
        .map_err(|e| format!("Can't save the settings: {e}"))
}

#[tauri::command]
fn add_folder(app: AppHandle, state: State<Shared>, path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    let p = p.canonicalize().map_err(|e| format!("{path}: {e}"))?;
    {
        let mut s = state.settings.lock().unwrap();
        if !s.folders.contains(&p) {
            s.folders.push(p);
        }
        save(&s)?;
    }
    reload(&app, &state);
    Ok(())
}

#[tauri::command]
fn remove_folder(app: AppHandle, state: State<Shared>, path: String) -> Result<(), String> {
    {
        let mut s = state.settings.lock().unwrap();
        s.folders.retain(|f| f.to_string_lossy() != path);
        save(&s)?;
    }
    reload(&app, &state);
    Ok(())
}

#[tauri::command]
fn rescan(app: AppHandle, state: State<Shared>) {
    reload(&app, &state);
}

#[tauri::command]
fn set_enabled(state: State<Shared>, id: String, enabled: bool) -> Result<(), String> {
    let mut s = state.settings.lock().unwrap();
    s.disabled.retain(|d| *d != id);
    if !enabled {
        s.disabled.push(id);
    }
    save(&s)?;
    if let Some(lib) = state.library.write().unwrap().as_mut() {
        lib.apply(&s);
    }
    Ok(())
}

/// Move a dictionary up (-1) or down (+1) in the order its articles are shown.
#[tauri::command]
fn move_dict(state: State<Shared>, id: String, by: i32) -> Result<(), String> {
    let mut s = state.settings.lock().unwrap();
    let mut guard = state.library.write().unwrap();
    let Some(lib) = guard.as_mut() else {
        return Ok(());
    };
    let mut order = lib.order();
    if let Some(i) = order.iter().position(|o| *o == id) {
        let j = (i as i32 + by).clamp(0, order.len() as i32 - 1) as usize;
        let d = order.remove(i);
        order.insert(j, d);
    }
    s.order = order;
    save(&s)?;
    lib.apply(&s);
    Ok(())
}

fn mime(name: &str) -> &'static str {
    match kosha_core::render::ext(name).as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" | "spx" | "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" | "aac" => "audio/mp4",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "tif" | "tiff" => "image/tiff",
        _ => "application/octet-stream",
    }
}

/// The words given on the command line (flags dropped).
fn word_from_args(args: &[String]) -> Option<String> {
    let w = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    (!w.trim().is_empty()).then_some(w)
}

fn main() {
    let state: Shared = Arc::new(AppState {
        settings: Mutex::new(Settings::load()),
        first_word: Mutex::new(word_from_args(&std::env::args().collect::<Vec<_>>())),
        ..Default::default()
    });
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
            if let Some(word) = word_from_args(&argv) {
                let _ = app.emit("lookup", word);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state.clone())
        .manage(cards::Cards::load())
        .register_uri_scheme_protocol("kosha", |ctx, request| {
            let state = ctx.app_handle().state::<Shared>();
            let path = request.uri().path().trim_start_matches('/');
            let (dict, name) = path.split_once('/').unwrap_or((path, ""));
            let name = percent_decode_str(name).decode_utf8_lossy();
            let data = state
                .library
                .read()
                .unwrap()
                .as_ref()
                .and_then(|l| l.resource(dict, &name));
            match data {
                Some(bytes) => Response::builder()
                    .header(header::CONTENT_TYPE, mime(&name))
                    .body(bytes)
                    .unwrap(),
                None => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Vec::new())
                    .unwrap(),
            }
        })
        .invoke_handler(tauri::generate_handler![
            search,
            lookup,
            library,
            first_word,
            about,
            add_folder,
            remove_folder,
            rescan,
            set_enabled,
            move_dict,
            cards::cards,
            cards::card_add,
            cards::card_update,
            cards::card_review,
            cards::card_delete,
            cards::folder_add,
            cards::folder_rename,
            cards::folder_move,
            cards::folder_delete
        ])
        .setup(move |app| {
            reload(app.handle(), &state);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Kosha could not start");
}
