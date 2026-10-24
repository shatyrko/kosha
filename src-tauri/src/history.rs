//! Search history commands (kosha_core::history): every change is saved at once.

use std::path::PathBuf;
use std::sync::Mutex;

use kosha_core::history::{self, Entry};
use tauri::State;

pub struct History {
    path: PathBuf,
    entries: Mutex<Vec<Entry>>,
}

impl History {
    pub fn load() -> History {
        let path = history::default_path();
        let entries = Mutex::new(history::load(&path));
        History { path, entries }
    }

    fn change(&self, f: impl FnOnce(&mut Vec<Entry>)) -> Result<Vec<Entry>, String> {
        let mut e = self.entries.lock().unwrap();
        f(&mut e);
        history::save(&self.path, &e).map_err(|e| format!("Can't save the search history: {e}"))?;
        Ok(e.clone())
    }
}

#[tauri::command]
pub fn history(h: State<History>) -> Vec<Entry> {
    h.entries.lock().unwrap().clone()
}

#[tauri::command]
pub fn history_add(h: State<History>, word: String) -> Result<Vec<Entry>, String> {
    h.change(|e| history::add(e, &word))
}

#[tauri::command]
pub fn history_remove(h: State<History>, word: String) -> Result<Vec<Entry>, String> {
    h.change(|e| history::remove(e, &word))
}

#[tauri::command]
pub fn history_clear(h: State<History>) -> Result<Vec<Entry>, String> {
    h.change(|e| e.clear())
}
