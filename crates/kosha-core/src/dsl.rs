//! The structure of a DSL file.
//!
//! ```text
//! #NAME "English-Russian"            header lines, before the first article
//! #INDEX_LANGUAGE "English"
//! #CONTENTS_LANGUAGE "Russian"
//!
//! apple                             headword lines start at the left edge;
//! apples                            several in a row share one article
//!     [m1][trn]яблоко[/trn][/m]     body lines start with a tab or spaces
//!     @ apple tree                  a sub-entry inside the article, up to a line "@"
//!     [m2]яблоня[/m]
//!     @
//! ```
//! `{{...}}` is a comment (may span lines). `\` escapes the next character.

use std::io::{self, Read};

use crate::text::{Encoding, Lines};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    pub index_language: String,
    pub contents_language: String,
    pub source_code_page: String,
    pub include: Vec<String>,
}

/// One article found while scanning: its byte range and the raw headword lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawArticle {
    pub offset: u64,
    pub len: u64,
    pub headwords: Vec<String>,
    /// headwords of `@` sub-entries in the body
    pub sub_headwords: Vec<String>,
}

/// Removes `{{comments}}`; `in_comment` carries an open comment over to the next line.
pub fn strip_comments(line: &str, in_comment: &mut bool) -> String {
    let mut out = String::with_capacity(line.len());
    let b = line.as_bytes();
    let mut i = 0;
    let mut last = 0;
    while i < b.len() {
        if *in_comment {
            if b[i] == b'}' && b.get(i + 1) == Some(&b'}') {
                *in_comment = false;
                i += 2;
                last = i;
                continue;
            }
        } else if b[i] == b'\\' {
            i += 2;
            continue;
        } else if b[i] == b'{' && b.get(i + 1) == Some(&b'{') {
            out.push_str(&line[last..i]);
            *in_comment = true;
            i += 2;
            continue;
        }
        i += 1;
    }
    if !*in_comment {
        out.push_str(&line[last.min(line.len())..]);
    }
    out
}

fn header_value(line: &str) -> (String, String) {
    let line = line.trim_start_matches('#');
    let (k, v) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    (
        k.to_ascii_uppercase(),
        v.trim().trim_matches('"').to_string(),
    )
}

pub fn is_body(line: &str) -> bool {
    line.starts_with([' ', '\t'])
}

/// Scan a whole file. Returns the header and every article, in file order.
pub fn scan(reader: impl Read, enc: Encoding, bom: usize) -> io::Result<(Header, Vec<RawArticle>)> {
    let mut lines = Lines::new(reader, enc, bom)?;
    let mut enc = enc;
    let mut header = Header::default();
    let mut articles: Vec<RawArticle> = Vec::new();
    let mut in_header = true;
    let mut in_comment = false;
    // the article being collected, and whether its body has started
    let mut cur: Option<RawArticle> = None;
    let mut in_body = false;
    while let Some((off, bytes)) = lines.next_line()? {
        let raw = enc.decode(&bytes);
        let line = strip_comments(&raw, &mut in_comment);
        if in_header {
            let t = line.trim_start_matches('\u{feff}');
            if t.starts_with('#') {
                let (k, v) = header_value(t);
                match k.as_str() {
                    "NAME" => header.name = v,
                    "INDEX_LANGUAGE" => header.index_language = v,
                    "CONTENTS_LANGUAGE" => header.contents_language = v,
                    "INCLUDE" => header.include.push(v),
                    "SOURCE_CODE_PAGE" => {
                        if let (Encoding::Legacy(_), Some(e)) = (enc, Encoding::from_code_page(&v))
                        {
                            enc = e;
                        }
                        header.source_code_page = v;
                    }
                    _ => {}
                }
                continue;
            }
            if t.trim().is_empty() {
                continue;
            }
            in_header = false;
        }
        if line.trim().is_empty() {
            continue;
        }
        if is_body(&line) {
            if let Some(a) = cur.as_mut() {
                in_body = true;
                let t = line.trim();
                if let Some(sub) = t.strip_prefix('@') {
                    let sub = sub.trim();
                    if !sub.is_empty() {
                        a.sub_headwords.push(sub.to_string());
                    }
                }
            }
            continue;
        }
        // a headword line
        if in_body || cur.is_none() {
            if let Some(mut a) = cur.take() {
                a.len = off - a.offset;
                articles.push(a);
            }
            cur = Some(RawArticle {
                offset: off,
                len: 0,
                headwords: Vec::new(),
                sub_headwords: Vec::new(),
            });
            in_body = false;
        }
        cur.as_mut()
            .unwrap()
            .headwords
            .push(line.trim_end().to_string());
    }
    if let Some(mut a) = cur.take() {
        a.len = lines.position() - a.offset;
        articles.push(a);
    }
    Ok((header, articles))
}

/// An article's text split into its headword lines and body lines (comments removed).
pub fn split_article(text: &str) -> (Vec<String>, Vec<String>) {
    let mut heads = Vec::new();
    let mut body = Vec::new();
    let mut in_comment = false;
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let line = strip_comments(raw, &mut in_comment);
        if line.trim().is_empty() {
            continue;
        }
        if is_body(&line) {
            body.push(line.trim().to_string());
        } else if body.is_empty() {
            heads.push(line.trim_end().to_string());
        } else {
            break; // the next article (a range should never reach it)
        }
    }
    (heads, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "#NAME \"Test\"\n#INDEX_LANGUAGE \"English\"\n#CONTENTS_LANGUAGE \"Russian\"\n\napple\napples\n\t[m1]яблоко[/m]\n\t@ apple tree\n\t[m2]яблоня[/m]\n\t@\n\n{{a comment\nspanning}}\npear\n  груша\n";

    #[test]
    fn scan_sample() {
        let (h, a) = scan(SAMPLE.as_bytes(), Encoding::Utf8, 0).unwrap();
        assert_eq!(h.name, "Test");
        assert_eq!(
            (h.index_language.as_str(), h.contents_language.as_str()),
            ("English", "Russian")
        );
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].headwords, vec!["apple", "apples"]);
        assert_eq!(a[0].sub_headwords, vec!["apple tree"]);
        assert_eq!(a[1].headwords, vec!["pear"]);
        let first = &SAMPLE[a[0].offset as usize..(a[0].offset + a[0].len) as usize];
        assert!(first.starts_with("apple\n") && first.contains("яблоня"));
        let (heads, body) = split_article(first);
        assert_eq!(heads, vec!["apple", "apples"]);
        assert_eq!(body[0], "[m1]яблоко[/m]");
        assert_eq!(&SAMPLE[(a[1].offset + a[1].len) as usize..], "");
    }

    #[test]
    fn comments() {
        let mut c = false;
        assert_eq!(strip_comments("a{{x}}b", &mut c), "ab");
        assert_eq!(strip_comments("a{{x", &mut c), "a");
        assert!(c);
        assert_eq!(strip_comments("y}}b", &mut c), "b");
        assert_eq!(strip_comments(r"a\{{b", &mut c), r"a\{{b");
    }
}
