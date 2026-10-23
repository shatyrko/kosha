//! DSL article markup → HTML (the app) or plain text (launcher, terminal).
//!
//! Tags: `[b] [i] [u] [c] [c color] [sup] [sub] [*]` (secondary, can be hidden), `[m0]`–`[m9]`
//! (indent of the line), `[trn] [!trs]` (translation), `[ex]` (example), `[com]` (comment),
//! `[p]` (label, expanded from the dictionary's `_abrv.dsl`), `[t]` (transcription), `[lang ...]`,
//! `[s]file[/s]` (sound or picture), `[url]link[/url]`, `[ref]word[/ref]` and `<<word>>`
//! (cross-reference), `[br]`. `~` is the headword (`^~` with the case of its first letter changed),
//! `\x` the character x. Unknown tags are dropped.
//! DSL files often close tags out of order; the HTML is always properly nested.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Text(String),
    Open(String, String),
    Close(String),
    Media(String),
    Ref(String),
    Url(String),
    Break,
}

/// Article lines (comments already removed) → nodes per line, and the line's indent.
fn parse_line(line: &str, headword: &str) -> (u8, Vec<Node>) {
    let mut nodes = Vec::new();
    let mut text = String::new();
    let mut margin = None;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let flush = |text: &mut String, nodes: &mut Vec<Node>| {
        if !text.is_empty() {
            nodes.push(Node::Text(std::mem::take(text)));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' if i + 1 < chars.len() => {
                text.push(chars[i + 1]);
                i += 2;
            }
            '~' => {
                text.push_str(headword);
                i += 1;
            }
            // `^~`: the headword with the case of its first letter changed
            '^' if chars.get(i + 1) == Some(&'~') => {
                let mut hw = headword.chars();
                if let Some(f) = hw.next() {
                    if f.is_uppercase() {
                        text.extend(f.to_lowercase());
                    } else {
                        text.extend(f.to_uppercase());
                    }
                    text.push_str(hw.as_str());
                }
                i += 2;
            }
            '<' if chars.get(i + 1) == Some(&'<') => {
                if let Some(end) = find(&chars, i + 2, &['>', '>']) {
                    flush(&mut text, &mut nodes);
                    nodes.push(Node::Ref(unescape(&chars[i + 2..end])));
                    i = end + 2;
                } else {
                    text.push(c);
                    i += 1;
                }
            }
            '[' => {
                let Some(end) = (i + 1..chars.len()).find(|&k| chars[k] == ']') else {
                    text.push(c);
                    i += 1;
                    continue;
                };
                let tag: String = chars[i + 1..end].iter().collect();
                let tag = tag.trim();
                let (name, attr) = match tag.split_once(char::is_whitespace) {
                    Some((n, a)) => (n.to_ascii_lowercase(), a.trim().to_string()),
                    None => (tag.to_ascii_lowercase(), String::new()),
                };
                i = end + 1;
                // tags whose content is taken as it is
                if matches!(name.as_str(), "s" | "url" | "ref" | "video") {
                    let close: Vec<char> = format!("[/{name}]").chars().collect();
                    let stop = find_ci(&chars, i, &close).unwrap_or(chars.len());
                    let inner = unescape(&chars[i..stop]).trim().to_string();
                    i = (stop + close.len()).min(chars.len());
                    flush(&mut text, &mut nodes);
                    match name.as_str() {
                        "s" | "video" => nodes.push(Node::Media(inner)),
                        "url" => nodes.push(Node::Url(inner)),
                        _ => nodes.push(Node::Ref(inner)),
                    }
                    continue;
                }
                if let Some(n) = name.strip_prefix('m').and_then(|d| d.parse::<u8>().ok()) {
                    margin.get_or_insert(n.min(9));
                    continue;
                }
                flush(&mut text, &mut nodes);
                if name == "br" {
                    nodes.push(Node::Break);
                } else if let Some(n) = name.strip_prefix('/') {
                    nodes.push(Node::Close(n.to_string()));
                } else {
                    nodes.push(Node::Open(name, attr));
                }
            }
            _ => {
                text.push(c);
                i += 1;
            }
        }
    }
    flush(&mut text, &mut nodes);
    (margin.unwrap_or(1), nodes)
}

fn find(chars: &[char], from: usize, pat: &[char]) -> Option<usize> {
    (from..chars.len().saturating_sub(pat.len() - 1)).find(|&k| chars[k..k + pat.len()] == *pat)
}

fn find_ci(chars: &[char], from: usize, pat: &[char]) -> Option<usize> {
    (from..chars.len().saturating_sub(pat.len() - 1)).find(|&k| {
        chars[k..k + pat.len()]
            .iter()
            .zip(pat)
            .all(|(a, b)| a.to_ascii_lowercase() == *b)
    })
}

fn unescape(chars: &[char]) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            i += 1;
        }
        s.push(chars[i]);
        i += 1;
    }
    s
}

pub fn escape_html(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

/// What a tag becomes in HTML: element and class.
fn element(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "b" => ("b", ""),
        "i" => ("i", ""),
        "u" => ("u", ""),
        "sup" => ("sup", ""),
        "sub" => ("sub", ""),
        "c" => ("span", "c"),
        "*" => ("span", "sec"),
        "trn" | "trn1" | "!trs" | "!trn" => ("span", "trn"),
        "ex" => ("span", "ex"),
        "com" => ("span", "com"),
        "p" => ("abbr", "p"),
        "t" => ("span", "tr"),
        "lang" => ("span", "lang"),
        "'" => ("span", "stress"),
        _ => return None,
    })
}

fn color_style(attr: &str) -> Option<String> {
    let a = attr.trim().trim_matches('"');
    let ok =
        !a.is_empty() && a.len() <= 24 && a.chars().all(|c| c.is_ascii_alphanumeric() || c == '#');
    ok.then(|| format!(" style=\"color:{a}\""))
}

pub struct RenderCtx<'a> {
    /// shown for `~`
    pub headword: &'a str,
    /// `[p]` labels: abbreviation → expansion
    pub abbreviations: &'a HashMap<String, String>,
    /// URL of a resource file (sound / picture) of this dictionary
    pub resource_url: &'a dyn Fn(&str) -> String,
}

const AUDIO: &[&str] = &[
    "wav", "mp3", "ogg", "oga", "opus", "flac", "m4a", "aac", "spx",
];
const IMAGE: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "tif", "tiff",
];

pub fn ext(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Body lines → an HTML fragment (`<div class="m1">…</div>` per line).
pub fn to_html(lines: &[String], ctx: &RenderCtx) -> String {
    let mut out = String::new();
    for line in lines {
        let t = line.trim();
        if let Some(sub) = t.strip_prefix('@') {
            let sub = sub.trim();
            if sub.is_empty() {
                out.push_str("<div class=\"sub-end\"></div>");
            } else {
                out.push_str(&format!(
                    "<div class=\"sub-head\">{}</div>",
                    escape_html(sub)
                ));
            }
            continue;
        }
        let (margin, nodes) = parse_line(t, ctx.headword);
        out.push_str(&format!("<div class=\"m m{margin}\">"));
        // open DSL tags, in order: (name, closing html)
        let mut stack: Vec<(String, String, String)> = Vec::new(); // (name, open html, close html)
        for n in nodes {
            match n {
                Node::Text(s) => out.push_str(&escape_html(&s)),
                Node::Break => out.push_str("<br>"),
                Node::Open(name, attr) => {
                    let Some((el, class)) = element(&name) else {
                        continue;
                    };
                    let mut open = format!("<{el}");
                    if !class.is_empty() {
                        open.push_str(&format!(" class=\"{class}\""));
                    }
                    if name == "c" {
                        open.push_str(&color_style(&attr).unwrap_or_default());
                    }
                    if name == "p" {
                        // expansion of the label: known when the label text follows; filled below
                        open.push_str(" data-p=\"1\"");
                    }
                    open.push('>');
                    out.push_str(&open);
                    stack.push((name, open, format!("</{el}>")));
                }
                Node::Close(name) => {
                    let Some(pos) = stack.iter().rposition(|(n, _, _)| *n == name) else {
                        continue;
                    };
                    for (_, _, close) in stack[pos..].iter().rev() {
                        out.push_str(close);
                    }
                    let reopen: Vec<_> = stack.drain(pos..).skip(1).collect();
                    for (_, open, _) in &reopen {
                        out.push_str(open);
                    }
                    stack.extend(reopen);
                }
                Node::Media(name) => out.push_str(&media_html(&name, ctx)),
                Node::Ref(word) => out.push_str(&format!(
                    "<a class=\"ref\" href=\"#\" data-word=\"{0}\">{0}</a>",
                    escape_html(&word)
                )),
                Node::Url(u) => {
                    let safe = u.starts_with("http://") || u.starts_with("https://");
                    if safe {
                        out.push_str(&format!(
                            "<a class=\"url\" href=\"{0}\">{0}</a>",
                            escape_html(&u)
                        ));
                    } else {
                        out.push_str(&escape_html(&u));
                    }
                }
            }
        }
        for (_, _, close) in stack.iter().rev() {
            out.push_str(close);
        }
        out.push_str("</div>");
    }
    fill_abbreviations(&out, ctx.abbreviations)
}

fn media_html(name: &str, ctx: &RenderCtx) -> String {
    let e = ext(name);
    let url = escape_html(&(ctx.resource_url)(name));
    let label = escape_html(name);
    if AUDIO.contains(&e.as_str()) {
        format!("<button class=\"sound\" data-src=\"{url}\" title=\"Play {label}\" aria-label=\"Play sound\"></button>")
    } else if IMAGE.contains(&e.as_str()) {
        format!("<img class=\"pic\" src=\"{url}\" alt=\"{label}\" loading=\"lazy\">")
    } else {
        format!("<a class=\"file\" href=\"{url}\">{label}</a>")
    }
}

/// `<abbr class="p" data-p="1">adj</abbr>` → with `title` = the expansion from `_abrv.dsl`.
fn fill_abbreviations(html: &str, abbr: &HashMap<String, String>) -> String {
    const OPEN: &str = "<abbr class=\"p\" data-p=\"1\">";
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(p) = rest.find(OPEN) {
        out.push_str(&rest[..p]);
        let after = &rest[p + OPEN.len()..];
        let end = after.find("</abbr>").unwrap_or(after.len());
        let label = strip_tags(&after[..end]);
        let key = unescape_html(&label).trim().to_string();
        match abbr
            .get(&key)
            .or_else(|| abbr.get(key.trim_end_matches('.')))
        {
            Some(full) => out.push_str(&format!(
                "<abbr class=\"p\" title=\"{}\">",
                escape_html(full)
            )),
            None => out.push_str("<abbr class=\"p\">"),
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

fn strip_tags(s: &str) -> String {
    let mut o = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => o.push(c),
            _ => {}
        }
    }
    o
}

fn unescape_html(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Body lines → plain text, indented two spaces per margin level past the first.
pub fn to_text(lines: &[String], headword: &str) -> String {
    let mut out = Vec::new();
    for line in lines {
        let t = line.trim();
        if let Some(sub) = t.strip_prefix('@') {
            let sub = sub.trim();
            if !sub.is_empty() {
                out.push(format!("— {sub}"));
            }
            continue;
        }
        let (margin, nodes) = parse_line(t, headword);
        let mut s = "  ".repeat(margin.saturating_sub(1) as usize);
        for n in nodes {
            match n {
                Node::Text(x) => s.push_str(&x),
                Node::Ref(x) => s.push_str(&x),
                Node::Url(x) => s.push_str(&x),
                Node::Break => s.push(' '),
                Node::Media(_) | Node::Open(..) | Node::Close(_) => {}
            }
        }
        let s = s.trim_end().to_string();
        if !s.trim().is_empty() {
            out.push(s);
        }
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(abbr: &'a HashMap<String, String>, f: &'a dyn Fn(&str) -> String) -> RenderCtx<'a> {
        RenderCtx {
            headword: "apple",
            abbreviations: abbr,
            resource_url: f,
        }
    }

    #[test]
    fn html_basic() {
        let abbr = HashMap::from([("n".to_string(), "noun".to_string())]);
        let f = |n: &str| format!("res://{n}");
        let lines = vec![
            "[m1][p]n[/p] [b]~[/b] [c red]red[/c] [s]apple.wav[/s]".to_string(),
            "[m2][trn]яблоко[/trn], see <<pear>>, \\[x\\]".to_string(),
        ];
        let h = to_html(&lines, &ctx(&abbr, &f));
        assert!(
            h.contains(
                "<div class=\"m m1\"><abbr class=\"p\" title=\"noun\">n</abbr> <b>apple</b>"
            ),
            "{h}"
        );
        assert!(h.contains("<span class=\"c\" style=\"color:red\">red</span>"));
        assert!(h.contains("data-src=\"res://apple.wav\""));
        assert!(h.contains("<div class=\"m m2\"><span class=\"trn\">яблоко</span>"));
        assert!(h.contains("data-word=\"pear\">pear</a>, [x]</div>"));
    }

    #[test]
    fn misnested_tags_are_repaired() {
        let abbr = HashMap::new();
        let f = |n: &str| n.to_string();
        let h = to_html(&["[b][i]x[/b]y[/i] z [/u]".to_string()], &ctx(&abbr, &f));
        assert_eq!(h, "<div class=\"m m1\"><b><i>x</i></b><i>y</i> z </div>");
    }

    #[test]
    fn no_html_injection() {
        let abbr = HashMap::new();
        let f = |n: &str| n.to_string();
        let h = to_html(&["<script>alert(1)</script> [c \"x;background:url(y)\"]a[/c] [url]javascript:x[/url]".to_string()], &ctx(&abbr, &f));
        assert!(
            !h.contains("<script") && !h.contains("background") && !h.contains("href=\"javascript"),
            "{h}"
        );
    }

    #[test]
    fn text() {
        let lines = vec![
            "[m1][b]1.[/b] яблоко".to_string(),
            "[m3][ex]an ~ a day[/ex]".to_string(),
            "@ apple tree".to_string(),
        ];
        assert_eq!(
            to_text(&lines, "apple"),
            "1. яблоко\n    an apple a day\n— apple tree"
        );
    }
}
