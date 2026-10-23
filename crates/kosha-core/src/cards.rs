//! Word cards for learning: words saved from articles, sorted into a tree of folders.
//!
//! A card keeps a copy of the article it was saved from (HTML and plain text), so it still works
//! when that dictionary is switched off or removed. Everything is one JSON file in Kosha's data
//! folder (`cards.json`), written atomically after every change.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Folder {
    pub id: String,
    pub name: String,
    /// `None`: at the top level
    #[serde(default)]
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Card {
    pub id: String,
    /// the word as it was looked up
    pub word: String,
    /// `None`: not in a folder ("Unsorted")
    pub folder: Option<String>,
    /// when it was saved (seconds since 1970)
    pub added: u64,
    pub dict_id: String,
    pub dict_name: String,
    /// headword of the saved article
    pub headword: String,
    /// copy of the article
    pub html: String,
    pub text: String,
    /// your own note
    pub note: String,
    /// marked "Got it" in study mode
    pub known: bool,
    pub reviews: u32,
    pub last_review: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Deck {
    pub version: u32,
    pub folders: Vec<Folder>,
    pub cards: Vec<Card>,
}

/// Fields of a card that can be changed; `None` leaves a field as it is.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct CardPatch {
    /// `Some(None)` moves the card out of every folder
    pub folder: Option<Option<String>>,
    pub note: Option<String>,
    pub known: Option<bool>,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn new_id() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:x}{:04x}", t, c & 0xffff)
}

pub fn default_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("kosha")
        .join("cards.json")
}

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, msg.to_string())
}

impl Deck {
    /// The saved cards; an empty deck when there is no file yet.
    pub fn load(path: &Path) -> io::Result<Deck> {
        match fs::read_to_string(path) {
            Ok(s) => {
                serde_json::from_str(&s).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Deck {
                version: 1,
                ..Default::default()
            }),
            Err(e) => Err(e),
        }
    }

    /// Write to a temporary file next to it, then rename: a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(d) = path.parent() {
            fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)? + "\n")?;
        fs::rename(tmp, path)
    }

    fn folder_exists(&self, id: &str) -> bool {
        self.folders.iter().any(|f| f.id == id)
    }

    /// `id` and every folder inside it, at any depth.
    pub fn subtree(&self, id: &str) -> Vec<String> {
        let mut out = vec![id.to_string()];
        let mut i = 0;
        while i < out.len() {
            let cur = out[i].clone();
            out.extend(
                self.folders
                    .iter()
                    .filter(|f| f.parent.as_deref() == Some(cur.as_str()))
                    .map(|f| f.id.clone()),
            );
            i += 1;
        }
        out
    }

    pub fn add_folder(&mut self, name: &str, parent: Option<&str>) -> io::Result<Folder> {
        let name = name.trim();
        if name.is_empty() {
            return Err(invalid("A folder needs a name"));
        }
        if let Some(p) = parent {
            if !self.folder_exists(p) {
                return Err(invalid("That folder no longer exists"));
            }
        }
        let f = Folder {
            id: new_id(),
            name: name.to_string(),
            parent: parent.map(str::to_string),
        };
        self.folders.push(f.clone());
        Ok(f)
    }

    pub fn rename_folder(&mut self, id: &str, name: &str) -> io::Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(invalid("A folder needs a name"));
        }
        let f = self
            .folders
            .iter_mut()
            .find(|f| f.id == id)
            .ok_or_else(|| invalid("No such folder"))?;
        f.name = name.to_string();
        Ok(())
    }

    /// Put a folder inside another one (`None`: top level). Refused when that would put it inside itself.
    pub fn move_folder(&mut self, id: &str, parent: Option<&str>) -> io::Result<()> {
        if let Some(p) = parent {
            if !self.folder_exists(p) {
                return Err(invalid("That folder no longer exists"));
            }
            if self.subtree(id).iter().any(|s| s == p) {
                return Err(invalid("A folder can't go inside itself"));
            }
        }
        let f = self
            .folders
            .iter_mut()
            .find(|f| f.id == id)
            .ok_or_else(|| invalid("No such folder"))?;
        f.parent = parent.map(str::to_string);
        Ok(())
    }

    /// Delete a folder. With `keep_contents`, its cards and sub-folders move up to its parent;
    /// otherwise they are deleted with it.
    pub fn delete_folder(&mut self, id: &str, keep_contents: bool) -> io::Result<()> {
        let parent = self
            .folders
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| invalid("No such folder"))?
            .parent
            .clone();
        if keep_contents {
            for f in self
                .folders
                .iter_mut()
                .filter(|f| f.parent.as_deref() == Some(id))
            {
                f.parent = parent.clone();
            }
            for c in self
                .cards
                .iter_mut()
                .filter(|c| c.folder.as_deref() == Some(id))
            {
                c.folder = parent.clone();
            }
            self.folders.retain(|f| f.id != id);
        } else {
            let gone = self.subtree(id);
            self.folders.retain(|f| !gone.contains(&f.id));
            self.cards
                .retain(|c| !c.folder.as_ref().is_some_and(|f| gone.contains(f)));
        }
        Ok(())
    }

    /// Save a card; the same word from the same dictionary in the same folder is not saved twice
    /// (the existing card is returned).
    pub fn add_card(&mut self, mut card: Card) -> io::Result<Card> {
        if let Some(f) = &card.folder {
            if !self.folder_exists(f) {
                return Err(invalid("That folder no longer exists"));
            }
        }
        if let Some(c) = self.cards.iter().find(|c| {
            c.word.to_lowercase() == card.word.to_lowercase()
                && c.dict_id == card.dict_id
                && c.folder == card.folder
        }) {
            return Ok(c.clone());
        }
        card.id = new_id();
        card.added = now();
        self.cards.push(card.clone());
        Ok(card)
    }

    pub fn update_card(&mut self, id: &str, patch: CardPatch) -> io::Result<Card> {
        if let Some(Some(f)) = &patch.folder {
            if !self.folder_exists(f) {
                return Err(invalid("That folder no longer exists"));
            }
        }
        let c = self
            .cards
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| invalid("No such card"))?;
        if let Some(f) = patch.folder {
            c.folder = f;
        }
        if let Some(n) = patch.note {
            c.note = n;
        }
        if let Some(k) = patch.known {
            c.known = k;
        }
        Ok(c.clone())
    }

    /// One study answer: known or not, counted, with the time.
    pub fn review(&mut self, id: &str, known: bool) -> io::Result<Card> {
        let c = self
            .cards
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| invalid("No such card"))?;
        c.known = known;
        c.reviews += 1;
        c.last_review = Some(now());
        Ok(c.clone())
    }

    pub fn delete_card(&mut self, id: &str) -> io::Result<()> {
        let n = self.cards.len();
        self.cards.retain(|c| c.id != id);
        if self.cards.len() == n {
            return Err(invalid("No such card"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(word: &str, folder: Option<&str>) -> Card {
        Card {
            word: word.into(),
            folder: folder.map(Into::into),
            dict_id: "d".into(),
            ..Default::default()
        }
    }

    #[test]
    fn folders_and_cards() {
        let mut d = Deck::default();
        let en = d.add_folder("English", None).unwrap();
        let verbs = d.add_folder("Verbs", Some(&en.id)).unwrap();
        let irregular = d.add_folder("Irregular", Some(&verbs.id)).unwrap();
        assert_eq!(d.subtree(&en.id).len(), 3);
        let a = d.add_card(card("go", Some(&irregular.id))).unwrap();
        // the same word in the same folder is not saved twice
        assert_eq!(
            d.add_card(card("Go", Some(&irregular.id))).unwrap().id,
            a.id
        );
        d.add_card(card("apple", None)).unwrap();
        assert_eq!(d.cards.len(), 2);
        // no folder inside itself
        assert!(d.move_folder(&en.id, Some(&irregular.id)).is_err());
        // deleting "Verbs" but keeping its contents: Irregular moves up to English
        d.delete_folder(&verbs.id, true).unwrap();
        assert_eq!(
            d.folders
                .iter()
                .find(|f| f.id == irregular.id)
                .unwrap()
                .parent
                .as_deref(),
            Some(en.id.as_str())
        );
        // deleting English with its contents takes the "go" card too
        d.delete_folder(&en.id, false).unwrap();
        assert!(d.folders.is_empty());
        assert_eq!(d.cards.len(), 1);
        let c = d.review(&d.cards[0].id.clone(), true).unwrap();
        assert!(c.known && c.reviews == 1);
    }

    #[test]
    fn save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("cards.json");
        assert_eq!(Deck::load(&p).unwrap().cards.len(), 0);
        let mut d = Deck::default();
        let f = d.add_folder("Food", None).unwrap();
        d.add_card(card("apple", Some(&f.id))).unwrap();
        d.save(&p).unwrap();
        let back = Deck::load(&p).unwrap();
        assert_eq!(back, d);
    }
}
