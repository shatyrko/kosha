//! Commands for the word cards (kosha_core::cards): every change is saved at once and the page
//! gets the whole deck back (it is small).
//!
//! If `cards.json` can't be read, nothing is ever written over it: changes are refused with the
//! reason, so a damaged file can still be repaired by hand.

use std::path::PathBuf;
use std::sync::Mutex;

use kosha_core::cards::{Card, CardPatch, Deck};
use tauri::State;

use crate::{resource_url, Shared};

pub struct Cards {
    path: PathBuf,
    deck: Mutex<Result<Deck, String>>,
}

impl Cards {
    pub fn load() -> Cards {
        let path = kosha_core::cards::default_path();
        let deck = Deck::load(&path)
            .map_err(|e| format!("Can't read your cards ({}): {e}", path.display()));
        Cards {
            path,
            deck: Mutex::new(deck),
        }
    }

    /// Change the deck and save it; the deck as it is afterwards.
    fn change(&self, f: impl FnOnce(&mut Deck) -> std::io::Result<()>) -> Result<Deck, String> {
        let mut guard = self.deck.lock().unwrap();
        let deck = guard.as_mut().map_err(|e| e.clone())?;
        let before = deck.clone();
        f(deck).map_err(|e| e.to_string())?;
        if let Err(e) = deck.save(&self.path) {
            *deck = before;
            return Err(format!("Can't save your cards: {e}"));
        }
        Ok(deck.clone())
    }
}

#[tauri::command]
pub fn cards(cards: State<Cards>) -> Result<Deck, String> {
    cards.deck.lock().unwrap().clone()
}

/// Save a word from one dictionary as a card; the article is copied from the dictionary here.
#[tauri::command]
pub fn card_add(
    state: State<Shared>,
    cards: State<Cards>,
    word: String,
    dict_id: String,
    folder: Option<String>,
) -> Result<Deck, String> {
    let lib = state.library.read().unwrap();
    let lib = lib.as_ref().ok_or("The dictionaries are still opening")?;
    let arts = lib.lookup_in(&dict_id, &word, &resource_url);
    let first = arts
        .first()
        .ok_or_else(|| format!("“{word}” is no longer in that dictionary"))?;
    let card = Card {
        word: word.clone(),
        folder,
        dict_id: dict_id.clone(),
        dict_name: first.dict_name.clone(),
        headword: first.headwords.first().cloned().unwrap_or(word),
        // homonyms: every article of this dictionary for the word, one after another
        html: arts
            .iter()
            .map(|a| a.html.as_str())
            .collect::<Vec<_>>()
            .join("<hr class=\"homonym\">"),
        text: arts
            .iter()
            .map(|a| a.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n"),
        ..Default::default()
    };
    cards.change(|d| d.add_card(card).map(|_| ()))
}

#[tauri::command]
pub fn card_update(
    cards: State<Cards>,
    id: String,
    patch: serde_json::Value,
) -> Result<Deck, String> {
    // `folder: null` in the page means "out of every folder": tell it apart from "not given"
    let mut p: CardPatch = serde_json::from_value(patch.clone()).map_err(|e| e.to_string())?;
    if patch.get("folder").is_some_and(|v| v.is_null()) {
        p.folder = Some(None);
    }
    cards.change(|d| d.update_card(&id, p).map(|_| ()))
}

#[tauri::command]
pub fn card_review(cards: State<Cards>, id: String, known: bool) -> Result<Deck, String> {
    cards.change(|d| d.review(&id, known).map(|_| ()))
}

#[tauri::command]
pub fn card_delete(cards: State<Cards>, id: String) -> Result<Deck, String> {
    cards.change(|d| d.delete_card(&id))
}

#[tauri::command]
pub fn folder_add(
    cards: State<Cards>,
    name: String,
    parent: Option<String>,
) -> Result<Deck, String> {
    cards.change(|d| d.add_folder(&name, parent.as_deref()).map(|_| ()))
}

#[tauri::command]
pub fn folder_rename(cards: State<Cards>, id: String, name: String) -> Result<Deck, String> {
    cards.change(|d| d.rename_folder(&id, &name))
}

#[tauri::command]
pub fn folder_move(
    cards: State<Cards>,
    id: String,
    parent: Option<String>,
) -> Result<Deck, String> {
    cards.change(|d| d.move_folder(&id, parent.as_deref()))
}

#[tauri::command]
pub fn folder_delete(cards: State<Cards>, id: String, keep_contents: bool) -> Result<Deck, String> {
    cards.change(|d| d.delete_folder(&id, keep_contents))
}
