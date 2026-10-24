//! Apple Dictionary bundles (`X.dictionary`), the format of macOS's Dictionary app; ABBYY Lingvo
//! for Mac shipped its dictionaries in it.
//!
//! ```text
//! X.dictionary/Contents/Info.plist   name (CFBundleDisplayName), bundle name with "En-Ru" etc.
//! X.dictionary/Contents/Body.data    the articles:
//!     0x40  u32 LE   length of the data that follows
//!     0x44  blocks:  u32 size (of what follows) · u32 · u32 unpacked size · zlib data
//!                    each unpacked block is one or more <d:entry …>…</d:entry> (XHTML)
//! ```
//! The other files (KeyText.index, EntryID.index: Apple's tries) are not needed: Kosha indexes
//! the `d:title` of every entry itself, and reads an article by decompressing its block, in place.
//! Article markup is turned into Kosha's own HTML through a whitelist (nothing else gets through).

use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::dsl::Header;
use crate::render::escape_html;

/// A `.dictionary` folder with the articles file inside.
pub fn is_bundle(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("dictionary"))
        && body_path(path).is_file()
}

pub fn body_path(bundle: &Path) -> PathBuf {
    bundle.join("Contents").join("Body.data")
}

/// The value of `<key>K</key><string>…</string>` in an Info.plist.
fn plist_string(plist: &str, key: &str) -> Option<String> {
    let k = format!("<key>{key}</key>");
    let rest = &plist[plist.find(&k)? + k.len()..];
    let s = rest.find("<string>")?;
    // the next value must be this key's
    if rest[..s].contains("<key>") {
        return None;
    }
    let rest = &rest[s + 8..];
    let e = rest.find("</string>")?;
    Some(decode_entities(&rest[..e]).trim().to_string())
}

fn language(code: &str) -> &'static str {
    match code.to_ascii_lowercase().as_str() {
        "en" => "English",
        "ru" => "Russian",
        "uk" | "ua" => "Ukrainian",
        "de" => "German",
        "fr" => "French",
        "es" => "Spanish",
        "it" => "Italian",
        "pt" => "Portuguese",
        "pl" => "Polish",
        "zh" | "ch" => "Chinese",
        "ja" | "jp" => "Japanese",
        "la" => "Latin",
        "kk" | "kz" => "Kazakh",
        "tt" => "Tatar",
        _ => "",
    }
}

/// Name and languages from Info.plist ("… Ru-En" in the bundle name or identifier).
pub fn header(bundle: &Path) -> Header {
    let plist = fs::read_to_string(bundle.join("Contents").join("Info.plist")).unwrap_or_default();
    let name = plist_string(&plist, "CFBundleDisplayName")
        .or_else(|| plist_string(&plist, "CFBundleName"))
        .unwrap_or_else(|| {
            bundle
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
    let mut h = Header {
        name,
        ..Default::default()
    };
    let hay = [
        plist_string(&plist, "CFBundleName").unwrap_or_default(),
        plist_string(&plist, "CFBundleIdentifier").unwrap_or_default(),
        bundle
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
    ];
    if let Some((a, c)) = hay.iter().find_map(|s| languages(s)) {
        h.index_language = a.to_string();
        h.contents_language = c.to_string();
    }
    h
}

/// "Ru-En" anywhere in the text, or "EnRu" at its end ("LVLawEnRu"): (from, to) languages.
fn languages(s: &str) -> Option<(&'static str, &'static str)> {
    let c: Vec<char> = s.chars().collect();
    let pair = |a: &[char], b: &[char]| -> Option<(&'static str, &'static str)> {
        if !a[0].is_ascii_uppercase() || !b[0].is_ascii_uppercase() {
            return None;
        }
        let (x, y) = (
            language(&a.iter().collect::<String>()),
            language(&b.iter().collect::<String>()),
        );
        (!x.is_empty() && !y.is_empty()).then_some((x, y))
    };
    let dashed = c
        .windows(5)
        .find_map(|w| (w[2] == '-').then(|| pair(&w[..2], &w[3..])).flatten());
    dashed.or_else(|| {
        (c.len() >= 4)
            .then(|| pair(&c[c.len() - 4..c.len() - 2], &c[c.len() - 2..]))
            .flatten()
    })
}

/// Every article: (offset of its block in Body.data, block length, title).
pub fn scan(body: &Path) -> io::Result<Vec<(u64, u32, String)>> {
    let mut r = BufReader::with_capacity(1 << 20, File::open(body)?);
    let mut head = [0u8; 0x44];
    r.read_exact(&mut head)?;
    let total = u32::from_le_bytes([head[0x40], head[0x41], head[0x42], head[0x43]]) as u64;
    let end = 0x44 + total;
    let mut pos = 0x44u64;
    let mut out = Vec::new();
    let mut buf = Vec::new();
    while pos + 12 <= end {
        let mut size = [0u8; 4];
        if r.read_exact(&mut size).is_err() {
            break;
        }
        let size = u32::from_le_bytes(size);
        if size < 8 {
            break; // the zero padding after the last block
        }
        buf.resize(size as usize, 0);
        r.read_exact(&mut buf)?;
        let block_len = size + 4;
        for e in entries(&unpack(&buf)?) {
            out.push((pos, block_len, title(e)));
        }
        pos += block_len as u64;
    }
    Ok(out)
}

/// A block as read from the file (with its size field) → its text.
pub fn block_text(block: &[u8]) -> io::Result<String> {
    if block.len() < 12 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "short block"));
    }
    unpack(&block[4..])
}

fn unpack(after_size: &[u8]) -> io::Result<String> {
    // u32 · u32 unpacked size · zlib data
    let data = after_size.get(8..).unwrap_or_default();
    let mut out = String::new();
    flate2::read::ZlibDecoder::new(data).read_to_string(&mut out)?;
    Ok(out)
}

/// The `<d:entry>…</d:entry>` pieces of a block.
pub fn entries(block: &str) -> impl Iterator<Item = &str> {
    block.split("</d:entry>").filter(|e| e.contains("<d:entry"))
}

/// Read one block at (offset, length) from Body.data.
pub fn read_block(file: &mut File, offset: u64, len: u32) -> io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; len as usize];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

pub fn title(entry: &str) -> String {
    let Some(start) = entry.find("<d:entry") else {
        return String::new();
    };
    let tag_end = entry[start..]
        .find('>')
        .map(|e| start + e)
        .unwrap_or(entry.len());
    let (_, attrs) = parse_tag(&entry[start + 1..tag_end]);
    attrs
        .iter()
        .find(|(k, _)| k == "d:title")
        .or_else(|| attrs.iter().find(|(k, _)| k == "id"))
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

/// The entry of a block whose title is `title` (a block can hold several).
pub fn entry_titled<'a>(block: &'a str, wanted: &str) -> Option<&'a str> {
    entries(block)
        .find(|e| title(e) == wanted)
        .or_else(|| entries(block).next())
}

// ── markup ───────────────────────────────────────────────────────────────

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(semi) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..semi];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            n if n.starts_with("#x") || n.starts_with("#X") => u32::from_str_radix(&n[2..], 16)
                .ok()
                .and_then(char::from_u32),
            n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `tag attr="v" …` (without < >) → (lower-case name, attributes with decoded values).
fn parse_tag(inner: &str) -> (String, Vec<(String, String)>) {
    let inner = inner.trim().trim_end_matches('/');
    let name_end = inner
        .find(|c: char| c.is_whitespace())
        .unwrap_or(inner.len());
    let name = inner[..name_end].to_ascii_lowercase();
    let mut attrs = Vec::new();
    let b = inner.as_bytes();
    let mut i = name_end;
    while i < b.len() {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let ks = i;
        while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        let key = inner[ks..i].to_ascii_lowercase();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut val = String::new();
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                let vs = i + 1;
                i = vs;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
                val = decode_entities(&inner[vs..i]);
                i += 1;
            } else {
                let vs = i;
                while i < b.len() && !b[i].is_ascii_whitespace() {
                    i += 1;
                }
                val = decode_entities(&inner[vs..i]);
            }
        }
        if !key.is_empty() {
            attrs.push((key, val));
        }
    }
    (name, attrs)
}

enum Tok<'a> {
    Text(&'a str),
    Open(String, Vec<(String, String)>, bool),
    Close(String),
}

fn tokens(xml: &str) -> Vec<Tok<'_>> {
    let mut out = Vec::new();
    let mut rest = xml;
    while !rest.is_empty() {
        match rest.find('<') {
            None => {
                out.push(Tok::Text(rest));
                break;
            }
            Some(i) => {
                if i > 0 {
                    out.push(Tok::Text(&rest[..i]));
                }
                rest = &rest[i..];
                if rest.starts_with("<!--") {
                    rest = rest.find("-->").map(|e| &rest[e + 3..]).unwrap_or("");
                    continue;
                }
                // the end of the tag, outside quotes
                let b = rest.as_bytes();
                let mut j = 1;
                let mut q = 0u8;
                while j < b.len() {
                    match b[j] {
                        b'"' | b'\'' if q == 0 => q = b[j],
                        c if c == q => q = 0,
                        b'>' if q == 0 => break,
                        _ => {}
                    }
                    j += 1;
                }
                let inner = &rest[1..j.min(rest.len())];
                rest = &rest[(j + 1).min(rest.len())..];
                if let Some(name) = inner.strip_prefix('/') {
                    out.push(Tok::Close(name.trim().to_ascii_lowercase()));
                } else if !inner.starts_with('!') && !inner.starts_with('?') {
                    let self_closing = inner.trim_end().ends_with('/');
                    let (name, attrs) = parse_tag(inner);
                    out.push(Tok::Open(name, attrs, self_closing));
                }
            }
        }
    }
    out
}

fn attr<'a>(attrs: &'a [(String, String)], k: &str) -> Option<&'a str> {
    attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
}

fn safe_color(c: &str) -> Option<&str> {
    let c = c.trim();
    (!c.is_empty() && c.len() <= 24 && c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '#'))
        .then_some(c)
}

/// Lingvo's label green: shown in Kosha's own label colour (readable in dark mode too).
fn is_label_green(c: &str) -> bool {
    matches!(
        c.to_ascii_lowercase().as_str(),
        "#007f00" | "#008000" | "green"
    )
}

fn level(attrs: &[(String, String)]) -> Option<u8> {
    let class = attr(attrs, "class")?;
    class
        .split_whitespace()
        .find_map(|c| {
            c.strip_prefix('L')
                .or_else(|| c.strip_prefix('l'))
                .and_then(|n| n.parse::<u8>().ok())
        })
        .map(|n| n.min(9))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The word an `x-dictionary:` link points to (`r:` an entry id, where `_` stands for a space).
fn link_word(href: &str) -> Option<String> {
    let rest = href.strip_prefix("x-dictionary:")?;
    let (kind, target) = rest.split_once(':').unwrap_or(("d", rest));
    let target = percent_decode(target);
    let word = if kind == "r" {
        target.replace('_', " ")
    } else {
        target
    };
    let word = word.trim().to_string();
    (!word.is_empty()).then_some(word)
}

const DROP_CONTENT: &[&str] = &[
    "script", "style", "head", "title", "object", "iframe", "svg", "math",
];

/// An entry's XHTML → Kosha's article HTML.
pub fn to_html(entry: &str) -> String {
    let mut out = String::new();
    // open elements: (source tag, closing html)
    let mut stack: Vec<(String, &'static str)> = Vec::new();
    let mut skip = 0usize; // inside <script> and the like
    for t in tokens(entry) {
        match t {
            Tok::Text(s) => {
                if skip > 0 {
                    continue;
                }
                let s = collapse(&decode_entities(s));
                // no space at the start of a line, nor two in a row
                let at_line_start = out.is_empty()
                    || out.ends_with("</div>")
                    || out.ends_with("<br>")
                    || out.ends_with("\">")
                        && out.rfind("<div").is_some_and(|d| !out[d..].contains("</"));
                let s = if at_line_start || out.ends_with(' ') {
                    s.trim_start()
                } else {
                    &s
                };
                if !s.is_empty() {
                    out.push_str(&escape_html(s));
                }
            }
            Tok::Open(name, attrs, self_closing) => {
                if DROP_CONTENT.contains(&name.as_str()) {
                    if !self_closing {
                        skip += 1;
                    }
                    continue;
                }
                if skip > 0 {
                    continue;
                }
                let (open, close): (String, &'static str) = match name.as_str() {
                    "br" => {
                        out.push_str("<br>");
                        continue;
                    }
                    "hr" => {
                        out.push_str("<hr class=\"homonym\">");
                        continue;
                    }
                    "div" | "p" => (
                        format!("<div class=\"m m{}\">", level(&attrs).unwrap_or(1)),
                        "</div>",
                    ),
                    "b" | "strong" => ("<b>".into(), "</b>"),
                    "i" | "em" => ("<i>".into(), "</i>"),
                    "u" => ("<u>".into(), "</u>"),
                    "sup" => ("<sup>".into(), "</sup>"),
                    "sub" => ("<sub>".into(), "</sub>"),
                    "font" => {
                        let color = attr(&attrs, "color").and_then(safe_color);
                        match (attr(&attrs, "title"), color) {
                            (Some(t), _) if !t.trim().is_empty() => (
                                format!("<abbr class=\"p\" title=\"{}\">", escape_html(t.trim())),
                                "</abbr>",
                            ),
                            (_, Some(c)) if is_label_green(c) => {
                                ("<span class=\"c\">".into(), "</span>")
                            }
                            (_, Some(c)) => (format!("<span style=\"color:{c}\">"), "</span>"),
                            _ => ("<span>".into(), "</span>"),
                        }
                    }
                    "span" => {
                        if attr(&attrs, "d:priority") == Some("2") {
                            ("<span class=\"sec\">".into(), "</span>")
                        } else if attr(&attrs, "class").is_some_and(|c| c.contains("transcription"))
                        {
                            ("<span class=\"tr\">".into(), "</span>")
                        } else {
                            ("<span>".into(), "</span>")
                        }
                    }
                    "a" => {
                        let href = attr(&attrs, "href").unwrap_or("");
                        if let Some(w) = link_word(href) {
                            (
                                format!(
                                    "<a class=\"ref\" href=\"#\" data-word=\"{}\">",
                                    escape_html(&w)
                                ),
                                "</a>",
                            )
                        } else if href.starts_with("http://") || href.starts_with("https://") {
                            (
                                format!("<a class=\"url\" href=\"{}\">", escape_html(href)),
                                "</a>",
                            )
                        } else {
                            ("<span>".into(), "</span>")
                        }
                    }
                    // anything else: the text stays, the element goes
                    _ => continue,
                };
                if self_closing {
                    continue;
                }
                out.push_str(&open);
                stack.push((name, close));
            }
            Tok::Close(name) => {
                if DROP_CONTENT.contains(&name.as_str()) {
                    skip = skip.saturating_sub(1);
                    continue;
                }
                if skip > 0 {
                    continue;
                }
                // close up to the matching element (badly nested markup is repaired)
                if let Some(pos) = stack.iter().rposition(|(n, _)| *n == name) {
                    for (_, c) in stack.drain(pos..).rev() {
                        out.push_str(c);
                    }
                }
            }
        }
    }
    for (_, c) in stack.drain(..).rev() {
        out.push_str(c);
    }
    tidy_end(out)
}

/// Runs of white space → one space (kept at the ends: it may separate two inline elements).
fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() && c != '\u{a0}' {
            space = true;
        } else {
            if space {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    if space {
        out.push(' ');
    }
    out
}

/// Drop the empty lines and line breaks Lingvo puts at the end of every entry.
fn tidy_end(mut html: String) -> String {
    loop {
        let t = html.trim_end();
        let cut = [
            "<br>",
            "<div class=\"m m0\"></div>",
            "<div class=\"m m1\"></div>",
        ]
        .iter()
        .find(|e| t.ends_with(*e))
        .map(|e| t.len() - e.len());
        match cut {
            Some(n) => html.truncate(n),
            None => break,
        }
    }
    html
}

/// An entry's XHTML → plain text, one line per block, indented two spaces per level past the first.
pub fn to_text(entry: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut skip = 0usize;
    let flush = |cur: &mut String, lines: &mut Vec<String>| {
        let t = cur.trim_end().to_string();
        if !t.trim().is_empty() {
            lines.push(t);
        }
        cur.clear();
    };
    for t in tokens(entry) {
        match t {
            Tok::Text(s) if skip == 0 => {
                let t = collapse(&decode_entities(s));
                // only the indentation so far: the text starts the line
                let t = if cur.trim().is_empty() {
                    t.trim_start().to_string()
                } else {
                    t
                };
                cur.push_str(&t);
            }
            Tok::Text(_) => {}
            Tok::Open(name, attrs, self_closing) => {
                if DROP_CONTENT.contains(&name.as_str()) {
                    if !self_closing {
                        skip += 1;
                    }
                } else if name == "br" || name == "div" || name == "p" {
                    flush(&mut cur, &mut lines);
                    if name != "br" {
                        cur.push_str(
                            &"  ".repeat(level(&attrs).unwrap_or(1).saturating_sub(1) as usize),
                        );
                    }
                }
            }
            Tok::Close(name) => {
                if DROP_CONTENT.contains(&name.as_str()) {
                    skip = skip.saturating_sub(1);
                } else if name == "div" || name == "p" {
                    flush(&mut cur, &mut lines);
                }
            }
        }
    }
    flush(&mut cur, &mut lines);
    lines
        .into_iter()
        .map(|l| {
            let indent = l.len() - l.trim_start().len();
            format!(
                "{}{}",
                &l[..indent],
                l.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = r##"<d:entry xmlns:d="http://www.apple.com/DTDs/DictionaryService-1.0.rng" id="stamp_it" d:title="stamp it">
<div class="L0"><b>stamp it</b></div>	<div class="L1"><font color="#007F00" title="сокращение от"> сокр. от </font>1) штамп &amp; <i>печать</i></div>	<div class="L2"><span d:priority="2">- <a STYLE="text-decoration: none" href="x-dictionary:r:time_stamp"> time stamp </a></span></div>
<script>alert(1)</script><div class="L1" onclick="x()"><font color="red">red</font> <img src="x.png"> <a href="javascript:x">no</a></div>	<div class="L0"></div><br />
<br />
</d:entry>"##;

    #[test]
    fn title_and_html() {
        assert_eq!(title(ENTRY), "stamp it");
        let h = to_html(ENTRY);
        assert!(
            h.starts_with("<div class=\"m m0\"><b>stamp it</b></div>"),
            "{h}"
        );
        assert!(h.contains("<abbr class=\"p\" title=\"сокращение от\">сокр. от </abbr>1) штамп &amp; <i>печать</i>"), "{h}");
        assert!(h.contains("<div class=\"m m2\"><span class=\"sec\">- <a class=\"ref\" href=\"#\" data-word=\"time stamp\">time stamp </a></span></div>"), "{h}");
        assert!(
            h.contains("<span style=\"color:red\">red</span> <span>no</span>"),
            "{h}"
        );
        assert!(h.contains("<span style=\"color:red\">red</span>"), "{h}");
        assert!(
            !h.contains("script")
                && !h.contains("alert")
                && !h.contains("onclick")
                && !h.contains("img")
                && !h.contains("javascript"),
            "{h}"
        );
        assert!(h.ends_with("</div>"), "trailing breaks removed: {h}");
    }

    #[test]
    fn text() {
        assert_eq!(
            to_text(ENTRY),
            "stamp it\nсокр. от 1) штамп & печать\n  - time stamp\nred no"
        );
    }

    #[test]
    fn entities_and_links() {
        assert_eq!(
            decode_entities("a &amp; b &#1092; &#x44; &bogus; &"),
            "a & b ф D &bogus; &"
        );
        assert_eq!(
            link_word("x-dictionary:r:punch_stamp").as_deref(),
            Some("punch stamp")
        );
        assert_eq!(
            link_word("x-dictionary:d:%D0%B4%D0%BE%D0%BC").as_deref(),
            Some("дом")
        );
    }
}
