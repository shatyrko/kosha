//! All dictionaries together: found in the folders of the settings, opened (indexed) in parallel,
//! searched and looked up as one.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::dictionary::{dict_id, Article, DictInfo, Dictionary};
use crate::normalize;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    /// searched for dictionaries, sub-folders included
    pub folders: Vec<PathBuf>,
    /// dictionary ids switched off
    pub disabled: Vec<String>,
    /// dictionary ids in the order articles are shown (others follow, by name)
    pub order: Vec<String>,
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("kosha")
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("kosha")
        .join("index")
}

impl Settings {
    pub fn path() -> PathBuf {
        config_dir().join("settings.json")
    }

    pub fn load() -> Settings {
        fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> io::Result<()> {
        let p = Self::path();
        if let Some(d) = p.parent() {
            fs::create_dir_all(d)?;
        }
        fs::write(p, serde_json::to_string_pretty(self)? + "\n")
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Hit {
    /// as shown (the first form found)
    pub word: String,
    pub key: String,
    /// how many dictionaries have it
    pub dicts: usize,
    /// found by similarity, not by its beginning
    pub fuzzy: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Failed {
    pub path: String,
    pub error: String,
}

pub struct Library {
    pub dicts: Vec<Dictionary>,
    pub failed: Vec<Failed>,
    disabled: HashSet<String>,
}

fn is_dictionary(p: &Path) -> bool {
    let n = p
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    (n.ends_with(".dsl") || n.ends_with(".dsl.dz")) && !n.contains("_abrv.dsl")
}

/// Dictionary files in these folders and their sub-folders.
pub fn find_files(folders: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut stack: Vec<PathBuf> = folders.to_vec();
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                let n = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                // resource folders next to a dictionary are not searched
                if !n.ends_with(".files") {
                    stack.push(p);
                }
            } else if is_dictionary(&p) {
                let c = p.canonicalize().unwrap_or(p);
                if seen.insert(c.clone()) {
                    out.push(c);
                }
            }
        }
    }
    out.sort();
    out
}

impl Library {
    /// Open every dictionary of these files (index built or loaded from `cache`), several at once.
    pub fn open(files: &[PathBuf], settings: &Settings, cache: Option<&Path>) -> Library {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .clamp(1, 8);
        let results: Vec<(PathBuf, io::Result<Dictionary>)> = std::thread::scope(|s| {
            let chunks: Vec<&[PathBuf]> =
                files.chunks(files.len().div_ceil(threads).max(1)).collect();
            let handles: Vec<_> = chunks
                .into_iter()
                .map(|chunk| {
                    s.spawn(move || {
                        chunk
                            .iter()
                            .map(|p| (p.clone(), Dictionary::open(p, cache)))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect()
        });
        let mut dicts = Vec::new();
        let mut failed = Vec::new();
        for (p, r) in results {
            match r {
                Ok(d) => dicts.push(d),
                Err(e) => failed.push(Failed {
                    path: p.to_string_lossy().into_owned(),
                    error: e.to_string(),
                }),
            }
        }
        let rank = |id: &str| {
            settings
                .order
                .iter()
                .position(|o| o == id)
                .unwrap_or(usize::MAX)
        };
        dicts.sort_by(|a, b| {
            rank(&a.id)
                .cmp(&rank(&b.id))
                .then_with(|| a.name().to_lowercase().cmp(&b.name().to_lowercase()))
        });
        Library {
            dicts,
            failed,
            disabled: settings.disabled.iter().cloned().collect(),
        }
    }

    /// The dictionaries of the saved settings.
    pub fn from_settings(settings: &Settings) -> Library {
        let cache = cache_dir();
        Library::open(&find_files(&settings.folders), settings, Some(&cache))
    }

    pub fn infos(&self) -> Vec<(DictInfo, bool)> {
        self.dicts
            .iter()
            .map(|d| (d.info(), !self.disabled.contains(&d.id)))
            .collect()
    }

    fn enabled(&self) -> impl Iterator<Item = &Dictionary> {
        self.dicts.iter().filter(|d| !self.disabled.contains(&d.id))
    }

    pub fn dict(&self, id: &str) -> Option<&Dictionary> {
        self.dicts.iter().find(|d| d.id == id)
    }

    /// Headwords starting with the query (the exact one first), then similar ones if few were found.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit> {
        let key = normalize::key(query);
        if key.is_empty() || limit == 0 {
            return Vec::new();
        }
        let mut found: BTreeMap<String, (String, HashSet<&str>)> = BTreeMap::new();
        for d in self.enabled() {
            // sorted, so the exact word (if any) comes first; `limit` different words per dictionary
            let ix = &d.index;
            let mut last: Option<&str> = None;
            let mut taken = 0;
            for w in &ix.words[ix.lower_bound(&key)..] {
                if !w.key.starts_with(&key) {
                    break;
                }
                if last != Some(w.key.as_str()) {
                    taken += 1;
                    if taken > limit {
                        break;
                    }
                    last = Some(&w.key);
                }
                found
                    .entry(w.key.clone())
                    .or_insert_with(|| (w.display.clone(), HashSet::new()))
                    .1
                    .insert(&d.id);
            }
        }
        let mut hits: Vec<Hit> = found
            .into_iter()
            .map(|(k, (word, ds))| Hit {
                word,
                dicts: ds.len(),
                fuzzy: false,
                key: k,
            })
            .collect();
        // the exact word first, then alphabetical
        hits.sort_by(|a, b| {
            (b.key == key)
                .cmp(&(a.key == key))
                .then_with(|| a.key.cmp(&b.key))
        });
        hits.truncate(limit);
        if hits.len() < 5 && key.chars().count() >= 3 {
            let have: HashSet<String> = hits.iter().map(|h| h.key.clone()).collect();
            for h in self.similar(&key, 10) {
                if hits.len() >= limit {
                    break;
                }
                if !have.contains(&h.key) {
                    hits.push(h);
                }
            }
        }
        hits
    }

    /// Headwords within a small edit distance of the key (typos), closest first.
    pub fn similar(&self, key: &str, limit: usize) -> Vec<Hit> {
        let q: Vec<char> = key.chars().collect();
        let max = if q.len() <= 4 { 1 } else { 2 };
        let qlen = key.len();
        let mut best: BTreeMap<(usize, String), (String, HashSet<&str>)> = BTreeMap::new();
        for d in self.enabled() {
            for w in &d.index.words {
                // a quick length check before the real comparison (bytes: up to 4 per character)
                if w.key.len() + max * 4 < qlen || w.key.len() > qlen + max * 4 {
                    continue;
                }
                let c: Vec<char> = w.key.chars().collect();
                if let Some(dist) = normalize::bounded_distance(&q, &c, max) {
                    if dist == 0 {
                        continue;
                    }
                    best.entry((dist, w.key.clone()))
                        .or_insert_with(|| (w.display.clone(), HashSet::new()))
                        .1
                        .insert(&d.id);
                }
            }
        }
        best.into_iter()
            .take(limit)
            .map(|((_, k), (word, ds))| Hit {
                word,
                key: k,
                dicts: ds.len(),
                fuzzy: true,
            })
            .collect()
    }

    /// Every dictionary's article for this word, in dictionary order.
    pub fn lookup(&self, word: &str, resource_url: &dyn Fn(&str, &str) -> String) -> Vec<Article> {
        let key = normalize::key(word);
        let mut out = Vec::new();
        for d in self.enabled() {
            let mut seen = HashSet::new();
            for w in d.index.exact(&key) {
                if seen.insert(w.article) {
                    if let Ok(a) = d.article(w.article, resource_url) {
                        out.push(a);
                    }
                }
            }
        }
        out
    }

    pub fn resource(&self, dict: &str, name: &str) -> Option<Vec<u8>> {
        self.dict(dict)?.resource(name)
    }
}

/// The id a dictionary file gets (for settings written by hand).
pub fn id_of(path: &Path) -> String {
    dict_id(&path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
}
