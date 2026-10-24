//! Search history: the words looked up, newest first, each once (looking a word up again moves it
//! to the top). Kept in `history.json` in Kosha's data folder, at most `MAX` words.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cards::now;

pub const MAX: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub word: String,
    /// when it was last looked up (seconds since 1970)
    pub at: u64,
    /// how many times
    #[serde(default = "one")]
    pub count: u32,
}

fn one() -> u32 {
    1
}

pub fn default_path() -> PathBuf {
    crate::cards::default_path().with_file_name("history.json")
}

pub fn load(path: &Path) -> Vec<Entry> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, entries: &[Entry]) -> io::Result<()> {
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string(entries)?)?;
    fs::rename(tmp, path)
}

/// Put a word at the top (same word in another case: replaced).
pub fn add(entries: &mut Vec<Entry>, word: &str) {
    let word = word.trim();
    if word.is_empty() {
        return;
    }
    let key = crate::normalize::key(word);
    let mut count = 0;
    entries.retain(|e| {
        let same = crate::normalize::key(&e.word) == key;
        if same {
            count = e.count;
        }
        !same
    });
    entries.insert(
        0,
        Entry {
            word: word.to_string(),
            at: now(),
            count: count + 1,
        },
    );
    entries.truncate(MAX);
}

pub fn remove(entries: &mut Vec<Entry>, word: &str) {
    let key = crate::normalize::key(word);
    entries.retain(|e| crate::normalize::key(&e.word) != key);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_once() {
        let mut h = Vec::new();
        add(&mut h, "apple");
        add(&mut h, "pear");
        add(&mut h, "Apple");
        assert_eq!(
            h.iter().map(|e| e.word.as_str()).collect::<Vec<_>>(),
            vec!["Apple", "pear"]
        );
        assert_eq!(h[0].count, 2);
        remove(&mut h, "PEAR");
        assert_eq!(h.len(), 1);
    }
}
