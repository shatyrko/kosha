//! StarDict dictionaries, read in place:
//!
//! ```text
//! X.ifo            text: bookname, wordcount, idxoffsetbits, sametypesequence, …
//! X.idx[.gz]       word \0, offset (u32 or u64 big-endian), size (u32): one per article
//! X.syn            word \0, u32 article number: more search forms of an article (optional)
//! X.dict[.dz]      the articles (dictzip: only the chunk an article is in is inflated)
//! ```
//! Articles are typed by `sametypesequence` (or by a type letter before each field): `h` HTML,
//! `m`/`t` plain text, `g` Pango markup, `x` XDXF. Markup goes through the same whitelist as the
//! Apple dictionaries' (appledict::to_html), plain text is escaped.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;

use crate::appledict;
use crate::dsl::Header;
use crate::render::escape_html;

/// The `.ifo` of a StarDict dictionary is what Kosha lists as the dictionary.
pub fn is_ifo(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("ifo"))
}

fn sibling(ifo: &Path, ext: &str) -> PathBuf {
    ifo.with_extension(ext)
}

/// The articles file: `X.dict.dz` or `X.dict`.
pub fn dict_path(ifo: &Path) -> PathBuf {
    let dz = sibling(ifo, "dict.dz");
    if dz.is_file() {
        dz
    } else {
        sibling(ifo, "dict")
    }
}

#[derive(Debug, Clone, Default)]
pub struct Ifo {
    pub bookname: String,
    pub description: String,
    pub sametypesequence: String,
    pub offset_bits: u8,
}

pub fn read_ifo(ifo: &Path) -> io::Result<Ifo> {
    let text = fs::read_to_string(ifo)?;
    if !text.starts_with("StarDict's dict ifo file") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a StarDict .ifo file",
        ));
    }
    let mut info = Ifo {
        offset_bits: 32,
        ..Default::default()
    };
    for line in text.lines().skip(1) {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim().to_string();
        match k.trim() {
            "bookname" => info.bookname = v,
            "description" => info.description = v,
            "sametypesequence" => info.sametypesequence = v,
            "idxoffsetbits" => info.offset_bits = if v == "64" { 64 } else { 32 },
            _ => {}
        }
    }
    Ok(info)
}

/// Name and languages: "mw-cologne (sa-en)" → Sanskrit → English.
pub fn header(ifo: &Path, info: &Ifo) -> Header {
    let name = if info.bookname.is_empty() {
        ifo.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        info.bookname.clone()
    };
    let mut h = Header {
        name,
        ..Default::default()
    };
    let lang = |c: &str| -> &'static str {
        match c.to_ascii_lowercase().as_str() {
            "sa" | "san" => "Sanskrit",
            "en" | "eng" => "English",
            "ru" | "rus" => "Russian",
            "de" | "deu" | "ger" => "German",
            "fr" | "fra" | "fre" => "French",
            "hi" | "hin" => "Hindi",
            "uk" | "ukr" => "Ukrainian",
            _ => "",
        }
    };
    if let Some(inner) = info
        .bookname
        .rsplit_once('(')
        .and_then(|(_, r)| r.split_once(')'))
        .map(|(i, _)| i)
    {
        if let Some((a, b)) = inner.split_once('-') {
            let (a, b) = (lang(a.trim()), lang(b.trim()));
            if !a.is_empty() && !b.is_empty() {
                h.index_language = a.into();
                h.contents_language = b.into();
            }
        }
    }
    h
}

fn read_maybe_gz(plain: &Path) -> io::Result<Vec<u8>> {
    let gz = PathBuf::from(format!("{}.gz", plain.to_string_lossy()));
    if plain.is_file() {
        fs::read(plain)
    } else if gz.is_file() {
        let mut out = Vec::new();
        GzDecoder::new(File::open(gz)?).read_to_end(&mut out)?;
        Ok(out)
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is missing", plain.display()),
        ))
    }
}

fn cstr(data: &[u8], pos: usize) -> Option<(String, usize)> {
    let end = pos + data.get(pos..)?.iter().position(|&b| b == 0)?;
    Some((
        String::from_utf8_lossy(&data[pos..end]).into_owned(),
        end + 1,
    ))
}

/// Every article: (word, offset in the articles file, size), in `.idx` order.
pub fn read_idx(ifo: &Path, info: &Ifo) -> io::Result<Vec<(String, u64, u32)>> {
    let data = read_maybe_gz(&sibling(ifo, "idx"))?;
    let wide = info.offset_bits == 64;
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let Some((word, p)) = cstr(&data, pos) else {
            break;
        };
        let need = if wide { 12 } else { 8 };
        let Some(b) = data.get(p..p + need) else {
            break;
        };
        let (off, size) = if wide {
            (
                u64::from_be_bytes(b[..8].try_into().unwrap()),
                u32::from_be_bytes(b[8..12].try_into().unwrap()),
            )
        } else {
            (
                u32::from_be_bytes(b[..4].try_into().unwrap()) as u64,
                u32::from_be_bytes(b[4..8].try_into().unwrap()),
            )
        };
        out.push((word, off, size));
        pos = p + need;
    }
    Ok(out)
}

/// More search forms: (word, article number). Empty when there is no `.syn`.
pub fn read_syn(ifo: &Path) -> io::Result<Vec<(String, u32)>> {
    let data = match read_maybe_gz(&sibling(ifo, "syn")) {
        Ok(d) => d,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let Some((word, p)) = cstr(&data, pos) else {
            break;
        };
        let Some(b) = data.get(p..p + 4) else { break };
        out.push((word, u32::from_be_bytes(b.try_into().unwrap())));
        pos = p + 4;
    }
    Ok(out)
}

/// Line breaks that are only the printed page's (Apte keeps one at every printed line: "--2 Of
/// <BR>Lakṣmī.") become spaces; a break stays before a new sense ("--2", "—", "1") or a link.
fn reflow(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(i) = ["<br>", "<br/>", "<br />"]
            .iter()
            .filter_map(|b| lower.find(b).map(|i| (i, b.len())))
            .min()
        else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..i.0]);
        rest = &rest[i.0 + i.1..];
        let next = rest.trim_start();
        let new_sense = next.starts_with("--")
            || next.starts_with('—')
            || next.starts_with('<')
            || next.starts_with(|c: char| c.is_ascii_digit());
        out.push_str(if new_sense { "<br>" } else { " " });
    }
    out
}

/// One field of an article → HTML.
fn field_html(kind: char, data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    let text = reflow(&text);
    match kind {
        'h' | 'g' | 'x' => appledict::to_html(&text),
        // plain text: kept as it is, lines kept
        _ => text
            .lines()
            .map(escape_html)
            .collect::<Vec<_>>()
            .join("<br>"),
    }
}

fn field_text(kind: char, data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    let text = reflow(&text);
    match kind {
        'h' | 'g' | 'x' => appledict::to_text(&text.replace("<BR>", "<br>")),
        _ => text,
    }
}

/// The fields of an article: with `sametypesequence` they follow each other untyped (the last one
/// takes the rest; text fields before it end with \0); without it, each starts with its type letter.
fn fields(info: &Ifo, data: &[u8]) -> Vec<(char, Vec<u8>)> {
    let mut out = Vec::new();
    let mut pos = 0;
    let types: Vec<char> = info.sametypesequence.chars().collect();
    let next = |kind: char, last: bool, pos: &mut usize| -> Option<Vec<u8>> {
        let rest = data.get(*pos..)?;
        if kind.is_ascii_uppercase() {
            // binary: u32 size first (or the rest, when last with sametypesequence)
            if last && !types.is_empty() {
                *pos = data.len();
                return Some(rest.to_vec());
            }
            let n = u32::from_be_bytes(rest.get(..4)?.try_into().ok()?) as usize;
            let v = rest.get(4..4 + n)?.to_vec();
            *pos += 4 + n;
            return Some(v);
        }
        if last && !types.is_empty() {
            *pos = data.len();
            return Some(rest.to_vec());
        }
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        *pos += end + 1;
        Some(rest[..end].to_vec())
    };
    if !types.is_empty() {
        for (i, &t) in types.iter().enumerate() {
            match next(t, i + 1 == types.len(), &mut pos) {
                Some(v) => out.push((t, v)),
                None => break,
            }
        }
    } else {
        while pos < data.len() {
            let t = data[pos] as char;
            pos += 1;
            match next(t, false, &mut pos) {
                Some(v) => out.push((t, v)),
                None => break,
            }
        }
    }
    out
}

pub fn article_html(info: &Ifo, data: &[u8]) -> String {
    fields(info, data)
        .iter()
        .filter(|(t, _)| matches!(t, 'h' | 'g' | 'x' | 'm' | 't' | 'l' | 'y'))
        .map(|(t, v)| field_html(*t, v))
        .collect::<Vec<_>>()
        .join("<br>")
}

pub fn article_text(info: &Ifo, data: &[u8]) -> String {
    fields(info, data)
        .iter()
        .filter(|(t, _)| matches!(t, 'h' | 'g' | 'x' | 'm' | 't' | 'l' | 'y'))
        .map(|(t, v)| field_text(*t, v))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idx_syn_and_articles() {
        let dir = tempfile::tempdir().unwrap();
        let ifo = dir.path().join("t.ifo");
        fs::write(&ifo, "StarDict's dict ifo file\nversion=2.4.2\nwordcount=2\nbookname=test (sa-en)\nsametypesequence=h\n").unwrap();
        let a1 = "वच् to speak<BR>pf. <i>uvāca</i>".as_bytes();
        let a2 = "अर्जुन <b>white</b>; N. of a hero".as_bytes();
        let mut dict = Vec::new();
        dict.extend_from_slice(a1);
        dict.extend_from_slice(a2);
        crate::source::write_dictzip(&dir.path().join("t.dict.dz"), &dict, 58315).unwrap();
        let mut idx = Vec::new();
        for (w, off, len) in [
            ("वच्", 0u32, a1.len() as u32),
            ("अर्जुन", a1.len() as u32, a2.len() as u32),
        ] {
            idx.extend_from_slice(w.as_bytes());
            idx.push(0);
            idx.extend_from_slice(&off.to_be_bytes());
            idx.extend_from_slice(&len.to_be_bytes());
        }
        fs::write(dir.path().join("t.idx"), &idx).unwrap();
        let mut syn = Vec::new();
        syn.extend_from_slice(b"arjuna\0");
        syn.extend_from_slice(&1u32.to_be_bytes());
        fs::write(dir.path().join("t.syn"), &syn).unwrap();

        let info = read_ifo(&ifo).unwrap();
        let h = header(&ifo, &info);
        assert_eq!(
            (h.index_language.as_str(), h.contents_language.as_str()),
            ("Sanskrit", "English")
        );
        let words = read_idx(&ifo, &info).unwrap();
        assert_eq!(words[1].0, "अर्जुन");
        assert_eq!(read_syn(&ifo).unwrap(), vec![("arjuna".to_string(), 1)]);
        let html = article_html(&info, a2);
        assert!(html.contains("<b>white</b>"), "{html}");
        assert_eq!(article_text(&info, a1), "वच् to speak pf. uvāca");
        assert_eq!(
            reflow("1 N. of Durgā.<BR>--2 Of<BR>Lakṣmī.<BR><a href=x>PDF</a>"),
            "1 N. of Durgā.<br>--2 Of Lakṣmī.<br><a href=x>PDF</a>"
        );
    }
}
