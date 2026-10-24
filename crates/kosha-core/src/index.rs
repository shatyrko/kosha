//! The headword index of one dictionary: where each article is in the file, and every searchable
//! form of every headword, sorted. Built by scanning the file once, saved in the cache folder, and
//! rebuilt when the file's size or modification time changes.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::appledict;
use crate::dsl::{self, Header};
use crate::headword;
use crate::normalize;
use crate::source::Source;
use crate::text::Encoding;

/// v2: the dictionary format is saved too (v1 indexes are rebuilt).
const MAGIC: &[u8; 8] = b"KOSHAIX\x02";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// ABBYY Lingvo DSL: `articles` are byte ranges of the uncompressed file
    Dsl,
    /// Apple Dictionary bundle: `articles` are blocks of Body.data; the top 16 bits of the
    /// offset are the entry's number inside its block (almost always 0)
    AppleDict,
}

pub const APPLE_OFFSET_MASK: u64 = (1 << 48) - 1;

/// The file whose size and time tell whether an index is still valid.
pub fn data_file(path: &Path) -> std::path::PathBuf {
    if appledict::is_bundle(path) {
        appledict::body_path(path)
    } else {
        path.to_path_buf()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// normalized search form
    pub key: String,
    /// as shown
    pub display: String,
    pub article: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    pub format: Format,
    pub header: Header,
    pub encoding: Encoding,
    /// byte offset and length of every article in the uncompressed file
    pub articles: Vec<(u64, u32)>,
    /// sorted by key, then display
    pub words: Vec<Word>,
    /// size and modification time (seconds) of the file the index was built from
    pub stamp: (u64, u64),
}

fn sort_words(words: &mut Vec<Word>) {
    words.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then_with(|| a.display.cmp(&b.display))
            .then(a.article.cmp(&b.article))
    });
    words.dedup_by(|a, b| a.key == b.key && a.article == b.article);
}

pub fn file_stamp(path: &Path) -> io::Result<(u64, u64)> {
    let m = fs::metadata(path)?;
    let t = m
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok((m.len(), t))
}

impl Index {
    pub fn build(path: &Path) -> io::Result<Index> {
        if appledict::is_bundle(path) {
            return Self::build_apple(path);
        }
        let stamp = file_stamp(path)?;
        let mut head = [0u8; 4096];
        let n = {
            let mut s = Source::stream(path)?;
            let mut n = 0;
            while n < head.len() {
                let k = s.read(&mut head[n..])?;
                if k == 0 {
                    break;
                }
                n += k;
            }
            n
        };
        let (enc, bom) = Encoding::sniff(&head[..n]);
        let (header, raw) = dsl::scan(Source::stream(path)?, enc, bom)?;
        // the header may name the code page of an old ANSI file
        let encoding = match (enc, Encoding::from_code_page(&header.source_code_page)) {
            (Encoding::Legacy(_), Some(e)) => e,
            _ => enc,
        };
        let mut articles = Vec::with_capacity(raw.len());
        let mut words = Vec::with_capacity(raw.len() * 2);
        for (i, a) in raw.iter().enumerate() {
            articles.push((a.offset, a.len.min(u32::MAX as u64) as u32));
            for h in a.headwords.iter().chain(&a.sub_headwords) {
                for (key, display) in headword::parse(h).forms {
                    words.push(Word {
                        key,
                        display,
                        article: i as u32,
                    });
                }
            }
        }
        sort_words(&mut words);
        Ok(Index {
            format: Format::Dsl,
            header,
            encoding,
            articles,
            words,
            stamp,
        })
    }

    /// An Apple Dictionary bundle: every entry's title, as it is (no DSL headword syntax).
    fn build_apple(bundle: &Path) -> io::Result<Index> {
        let body = appledict::body_path(bundle);
        let stamp = file_stamp(&body)?;
        let scanned = appledict::scan(&body)?;
        let mut articles = Vec::with_capacity(scanned.len());
        let mut words = Vec::with_capacity(scanned.len());
        let mut last_block = u64::MAX;
        let mut nth = 0u64;
        for (i, (off, len, title)) in scanned.into_iter().enumerate() {
            nth = if off == last_block { nth + 1 } else { 0 };
            last_block = off;
            articles.push((off | (nth.min(0xffff) << 48), len));
            let key = normalize::key(&title);
            if !key.is_empty() {
                words.push(Word {
                    key,
                    display: title,
                    article: i as u32,
                });
            }
        }
        sort_words(&mut words);
        Ok(Index {
            format: Format::AppleDict,
            header: appledict::header(bundle),
            encoding: Encoding::Utf8,
            articles,
            words,
            stamp,
        })
    }

    /// Index of words whose key starts with `prefix` (the first one).
    pub fn lower_bound(&self, prefix: &str) -> usize {
        self.words.partition_point(|w| w.key.as_str() < prefix)
    }

    /// All words with exactly this key.
    pub fn exact(&self, key: &str) -> &[Word] {
        let s = self.lower_bound(key);
        let e = s + self.words[s..].partition_point(|w| w.key == key);
        &self.words[s..e]
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(d) = path.parent() {
            fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("tmp");
        {
            let mut w = BufWriter::new(File::create(&tmp)?);
            w.write_all(MAGIC)?;
            w.write_all(&[match self.format {
                Format::Dsl => 0,
                Format::AppleDict => 1,
            }])?;
            put_u64(&mut w, self.stamp.0)?;
            put_u64(&mut w, self.stamp.1)?;
            let enc = match self.encoding {
                Encoding::Utf16Le => "utf-16le".to_string(),
                Encoding::Utf16Be => "utf-16be".to_string(),
                Encoding::Utf8 => "utf-8".to_string(),
                Encoding::Legacy(e) => e.name().to_string(),
            };
            put_str(&mut w, &enc)?;
            let h = &self.header;
            for s in [
                &h.name,
                &h.index_language,
                &h.contents_language,
                &h.source_code_page,
            ] {
                put_str(&mut w, s)?;
            }
            put_u64(&mut w, h.include.len() as u64)?;
            for s in &h.include {
                put_str(&mut w, s)?;
            }
            put_u64(&mut w, self.articles.len() as u64)?;
            for (o, l) in &self.articles {
                put_u64(&mut w, *o)?;
                w.write_all(&l.to_le_bytes())?;
            }
            put_u64(&mut w, self.words.len() as u64)?;
            let mut last_display = String::new();
            for wd in &self.words {
                put_str(&mut w, &wd.key)?;
                // the shown form is often the same as the previous word's: store it once
                if wd.display == last_display {
                    put_u64(&mut w, u64::MAX)?;
                } else {
                    put_str(&mut w, &wd.display)?;
                    last_display.clone_from(&wd.display);
                }
                w.write_all(&wd.article.to_le_bytes())?;
            }
            w.flush()?;
        }
        fs::rename(tmp, path)
    }

    /// The saved index, if it exists and was built from a file of this size and time.
    pub fn load(path: &Path, stamp: (u64, u64)) -> io::Result<Option<Index>> {
        let f = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let mut r = BufReader::with_capacity(1 << 20, f);
        let mut magic = [0u8; 8];
        if r.read_exact(&mut magic).is_err() || &magic != MAGIC {
            return Ok(None);
        }
        let mut f = [0u8; 1];
        r.read_exact(&mut f)?;
        let format = match f[0] {
            0 => Format::Dsl,
            1 => Format::AppleDict,
            _ => return Ok(None),
        };
        if (get_u64(&mut r)?, get_u64(&mut r)?) != stamp {
            return Ok(None);
        }
        let enc = get_str(&mut r)?;
        let encoding = match enc.as_str() {
            "utf-16le" => Encoding::Utf16Le,
            "utf-16be" => Encoding::Utf16Be,
            "utf-8" => Encoding::Utf8,
            other => match encoding_rs::Encoding::for_label(other.as_bytes()) {
                Some(e) => Encoding::Legacy(e),
                None => return Ok(None),
            },
        };
        let header = Header {
            name: get_str(&mut r)?,
            index_language: get_str(&mut r)?,
            contents_language: get_str(&mut r)?,
            source_code_page: get_str(&mut r)?,
            include: {
                let n = get_u64(&mut r)?;
                (0..n).map(|_| get_str(&mut r)).collect::<io::Result<_>>()?
            },
        };
        let n = get_u64(&mut r)? as usize;
        let mut articles = Vec::with_capacity(n);
        for _ in 0..n {
            let o = get_u64(&mut r)?;
            let mut l = [0u8; 4];
            r.read_exact(&mut l)?;
            articles.push((o, u32::from_le_bytes(l)));
        }
        let n = get_u64(&mut r)? as usize;
        let mut words = Vec::with_capacity(n);
        let mut last_display = String::new();
        for _ in 0..n {
            let key = get_str(&mut r)?;
            let len = get_u64(&mut r)?;
            if len != u64::MAX {
                last_display = get_bytes(&mut r, len)?;
            }
            let mut a = [0u8; 4];
            r.read_exact(&mut a)?;
            words.push(Word {
                key,
                display: last_display.clone(),
                article: u32::from_le_bytes(a),
            });
        }
        Ok(Some(Index {
            format,
            header,
            encoding,
            articles,
            words,
            stamp,
        }))
    }
}

fn put_u64(w: &mut impl Write, v: u64) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn put_str(w: &mut impl Write, s: &str) -> io::Result<()> {
    put_u64(w, s.len() as u64)?;
    w.write_all(s.as_bytes())
}

fn get_u64(r: &mut impl Read) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn get_bytes(r: &mut impl Read, len: u64) -> io::Result<String> {
    if len > 1 << 24 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "index entry too long",
        ));
    }
    let mut b = vec![0u8; len as usize];
    r.read_exact(&mut b)?;
    String::from_utf8(b).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn get_str(r: &mut impl Read) -> io::Result<String> {
    let len = get_u64(r)?;
    get_bytes(r, len)
}
