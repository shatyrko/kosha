//! One DSL dictionary: the file (read in place), its index, its abbreviations (`<name>_abrv.dsl`)
//! and its resource files — sounds and pictures — from `<name>.dsl.files.zip`, `<name>.files.zip`
//! or a `<name>.dsl.files` / `<name>.files` folder next to it.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;

use crate::dsl;
use crate::headword;
use crate::index::{file_stamp, Index};
use crate::render::{self, RenderCtx};
use crate::source::{without_dz, Source};
use crate::text::Encoding;

pub struct Dictionary {
    pub id: String,
    pub path: PathBuf,
    pub index: Index,
    source: Source,
    abbreviations: HashMap<String, String>,
    resources: Resources,
}

#[derive(Debug, Clone, Serialize)]
pub struct DictInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub index_language: String,
    pub contents_language: String,
    pub articles: usize,
    pub words: usize,
    pub abbreviations: usize,
    pub has_resources: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Article {
    pub dict_id: String,
    pub dict_name: String,
    /// headwords of the article, as shown
    pub headwords: Vec<String>,
    pub html: String,
    pub text: String,
}

enum Resources {
    None,
    Zip {
        archive: Mutex<zip::ZipArchive<File>>,
        names: HashMap<String, usize>,
    },
    Dir(PathBuf),
}

/// Stable short id of a dictionary file (FNV-1a of its path).
pub fn dict_id(path: &Path) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in path.to_string_lossy().as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn stem_variants(path: &Path) -> (PathBuf, PathBuf) {
    // x.dsl.dz → (x.dsl, x)
    let dsl = without_dz(path);
    let s = dsl.to_string_lossy();
    let bare = if s.to_ascii_lowercase().ends_with(".dsl") {
        PathBuf::from(&s[..s.len() - 4])
    } else {
        dsl.clone()
    };
    (dsl, bare)
}

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", p.to_string_lossy()))
}

impl Dictionary {
    /// Open a dictionary, using the saved index from `cache_dir` when it is still valid.
    pub fn open(path: &Path, cache_dir: Option<&Path>) -> io::Result<Dictionary> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let id = dict_id(&path);
        let stamp = file_stamp(&path)?;
        let cache = cache_dir.map(|d| d.join(format!("{id}.idx")));
        let index = match cache.as_deref().map(|c| Index::load(c, stamp)) {
            Some(Ok(Some(ix))) => ix,
            _ => {
                let ix = Index::build(&path)?;
                if let Some(c) = &cache {
                    // a cache that can't be written only costs time next start
                    let _ = ix.save(c);
                }
                ix
            }
        };
        let source = Source::open(&path)?;
        let (dsl_path, bare) = stem_variants(&path);
        let abbreviations = [
            with_suffix(&bare, "_abrv.dsl"),
            with_suffix(&bare, "_abrv.dsl.dz"),
        ]
        .iter()
        .find(|p| p.is_file())
        .map(|p| read_abbreviations(p).unwrap_or_default())
        .unwrap_or_default();
        let resources = open_resources(&dsl_path, &bare);
        Ok(Dictionary {
            id,
            path,
            index,
            source,
            abbreviations,
            resources,
        })
    }

    pub fn name(&self) -> String {
        if self.index.header.name.is_empty() {
            let (_, bare) = stem_variants(&self.path);
            bare.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            self.index.header.name.clone()
        }
    }

    pub fn info(&self) -> DictInfo {
        DictInfo {
            id: self.id.clone(),
            name: self.name(),
            path: self.path.to_string_lossy().into_owned(),
            index_language: self.index.header.index_language.clone(),
            contents_language: self.index.header.contents_language.clone(),
            articles: self.index.articles.len(),
            words: self.index.words.len(),
            abbreviations: self.abbreviations.len(),
            has_resources: !matches!(self.resources, Resources::None),
        }
    }

    /// The article's text as it is in the file.
    pub fn article_source(&self, article: u32) -> io::Result<String> {
        let (off, len) = self.index.articles[article as usize];
        let bytes = self.source.read_at(off, len as usize)?;
        Ok(self.index.encoding.decode(&bytes))
    }

    pub fn article(
        &self,
        article: u32,
        resource_url: &dyn Fn(&str, &str) -> String,
    ) -> io::Result<Article> {
        let text = self.article_source(article)?;
        let (heads, body) = dsl::split_article(&text);
        let headwords: Vec<String> = heads.iter().map(|h| headword::parse(h).display).collect();
        let first = headwords.first().cloned().unwrap_or_default();
        // `~` stands for the headword without its unsorted parts
        let tilde = heads.first().map(|h| strip_unsorted(h)).unwrap_or_default();
        let id = self.id.clone();
        let url = move |name: &str| resource_url(&id, name);
        let ctx = RenderCtx {
            headword: &tilde,
            abbreviations: &self.abbreviations,
            resource_url: &url,
        };
        Ok(Article {
            dict_id: self.id.clone(),
            dict_name: self.name(),
            html: render::to_html(&body, &ctx),
            text: render::to_text(&body, &tilde),
            headwords: if headwords.is_empty() {
                vec![first]
            } else {
                headwords
            },
        })
    }

    /// A sound or picture file of this dictionary.
    pub fn resource(&self, name: &str) -> Option<Vec<u8>> {
        let name = name.trim_start_matches(['/', '\\']);
        match &self.resources {
            Resources::None => None,
            Resources::Zip { archive, names } => {
                let i = *names.get(&name.to_lowercase())?;
                let mut a = archive.lock().ok()?;
                let mut f = a.by_index(i).ok()?;
                let mut buf = Vec::with_capacity(f.size() as usize);
                f.read_to_end(&mut buf).ok()?;
                Some(buf)
            }
            Resources::Dir(dir) => {
                if name.contains("..") {
                    return None;
                }
                std::fs::read(dir.join(name)).ok()
            }
        }
    }
}

fn strip_unsorted(h: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    let mut esc = false;
    for c in h.chars() {
        if esc {
            if depth == 0 {
                out.push(c);
            }
            esc = false;
            continue;
        }
        match c {
            '\\' => esc = true,
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            '(' | ')' if depth == 0 => {}
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn open_resources(dsl_path: &Path, bare: &Path) -> Resources {
    for z in [
        with_suffix(dsl_path, ".files.zip"),
        with_suffix(bare, ".files.zip"),
    ] {
        if let Ok(f) = File::open(&z) {
            if let Ok(archive) = zip::ZipArchive::new(f) {
                let mut names = HashMap::new();
                for i in 0..archive.len() {
                    if let Some(n) = archive.name_for_index(i) {
                        // files may sit in a folder inside the archive: also find them by base name
                        let lower = n.to_lowercase();
                        if let Some(base) = lower.rsplit(['/', '\\']).next() {
                            names.entry(base.to_string()).or_insert(i);
                        }
                        names.insert(lower, i);
                    }
                }
                return Resources::Zip {
                    archive: Mutex::new(archive),
                    names,
                };
            }
        }
    }
    for d in [with_suffix(dsl_path, ".files"), with_suffix(bare, ".files")] {
        if d.is_dir() {
            return Resources::Dir(d);
        }
    }
    Resources::None
}

/// `_abrv.dsl`: each headword is an abbreviation, its body the expansion.
fn read_abbreviations(path: &Path) -> io::Result<HashMap<String, String>> {
    let mut data = Vec::new();
    Source::stream(path)?.read_to_end(&mut data)?;
    let (enc, bom) = Encoding::sniff(&data);
    let (_, articles) = dsl::scan(&data[..], enc, bom)?;
    let mut map = HashMap::new();
    for a in articles {
        let end = (a.offset + a.len).min(data.len() as u64) as usize;
        let text = enc.decode(&data[a.offset as usize..end]);
        let (heads, body) = dsl::split_article(&text);
        let full = render::to_text(&body, "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        for h in heads {
            let k = headword::parse(&h).display;
            if !k.is_empty() && !full.is_empty() {
                map.insert(k, full.clone());
            }
        }
    }
    Ok(map)
}
