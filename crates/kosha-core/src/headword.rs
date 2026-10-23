//! DSL headwords: what is shown and what is searched.
//!
//! - `{...}` unsorted part: shown, not searched (`{to }run`, or markup like `{[i]}...{[/i]}`)
//! - `(...)` optional part: searched with and without it (`bring (in)` → "bring in", "bring")
//! - `\x`   the character x itself (`\(`, `\{`, `\~`, `\\`)
//! - `[tag]` markup is never part of the text

use crate::normalize;

/// At most this many search variants per headword (2^5 optional parts).
const MAX_VARIANTS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Headword {
    /// as shown above the article: braces dropped, optional parts kept in parentheses, no markup
    pub display: String,
    /// every searchable form: (normalized key, how that form is shown), without duplicate keys
    pub forms: Vec<(String, String)>,
    /// what `~` in the article stands for: no unsorted parts, no optional parts
    pub tilde: String,
}

impl Headword {
    pub fn keys(&self) -> Vec<&str> {
        self.forms.iter().map(|(k, _)| k.as_str()).collect()
    }
}

#[derive(Default)]
struct Part {
    key: String,
    shown: String,
    optional: bool,
}

pub fn parse(raw: &str) -> Headword {
    let mut display = String::new();
    let mut parts: Vec<Part> = vec![Part::default()];
    let mut unsorted = 0usize;
    let mut optional = false;
    let mut chars = raw.chars();
    let add = |parts: &mut Vec<Part>, c: char, searched: bool| {
        let p = parts.last_mut().unwrap();
        p.shown.push(c);
        if searched {
            p.key.push(c);
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    display.push(n);
                    add(&mut parts, n, unsorted == 0);
                }
            }
            '{' => unsorted += 1,
            '}' => unsorted = unsorted.saturating_sub(1),
            '[' => {
                // markup: skip to the closing bracket
                for n in chars.by_ref() {
                    if n == ']' {
                        break;
                    }
                }
            }
            '(' if unsorted == 0 && !optional => {
                optional = true;
                display.push('(');
                parts.push(Part {
                    optional: true,
                    ..Part::default()
                });
            }
            ')' if unsorted == 0 && optional => {
                optional = false;
                display.push(')');
                parts.push(Part::default());
            }
            c => {
                display.push(c);
                add(&mut parts, c, unsorted == 0);
            }
        }
    }
    // every combination of the optional parts: (key text, shown text)
    let mut variants = vec![(String::new(), String::new())];
    for p in &parts {
        let fixed = !p.optional || p.key.trim().is_empty() || variants.len() * 2 > MAX_VARIANTS;
        if fixed {
            if !p.optional || !p.key.trim().is_empty() {
                variants.iter_mut().for_each(|(k, s)| {
                    k.push_str(&p.key);
                    s.push_str(&p.shown);
                });
            }
        } else {
            let with: Vec<_> = variants
                .iter()
                .map(|(k, s)| (format!("{k}{}", p.key), format!("{s}{}", p.shown)))
                .collect();
            variants.extend(with);
        }
    }
    let tidy = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut forms: Vec<(String, String)> = Vec::new();
    for (k, s) in variants {
        let k = normalize::key(&k);
        if !k.is_empty() && !forms.iter().any(|(f, _)| *f == k) {
            forms.push((k, tidy(&s)));
        }
    }
    let tilde: String = parts
        .iter()
        .filter(|p| !p.optional)
        .map(|p| p.key.as_str())
        .collect();
    Headword {
        display: tidy(&display),
        forms,
        tilde: tidy(&tilde),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forms(raw: &str) -> Vec<(&'static str, &'static str)> {
        parse(raw)
            .forms
            .into_iter()
            .map(|(k, s)| (&*k.leak(), &*s.leak()))
            .collect()
    }

    #[test]
    fn plain() {
        assert_eq!(forms("Apple"), vec![("apple", "Apple")]);
        assert_eq!(parse("Apple").display, "Apple");
    }

    #[test]
    fn unsorted_part() {
        let h = parse("go{es}");
        assert_eq!(h.display, "goes");
        assert_eq!(forms("go{es}"), vec![("go", "goes")]);
        // markup in braces: only the tags are unsorted, the word between them is searched
        assert_eq!(parse("{[i]}to{[/i]} run").display, "to run");
        assert_eq!(parse("{[i]}to{[/i]} run").keys(), vec!["to run"]);
        assert_eq!(forms("{to }run"), vec![("run", "to run")]);
    }

    #[test]
    fn optional_parts() {
        let h = parse("bring (in)");
        assert_eq!(h.display, "bring (in)");
        assert_eq!(h.tilde, "bring");
        assert_eq!(parse("{to }run").tilde, "run");
        assert_eq!(
            forms("bring (in)"),
            vec![("bring", "bring"), ("bring in", "bring in")]
        );
        let h = parse("(to) look (up)");
        assert_eq!(h.forms.len(), 4);
        assert!(h.keys().contains(&"look") && h.keys().contains(&"to look up"));
    }

    #[test]
    fn escapes() {
        let h = parse(r"AT\&T \(company\)");
        assert_eq!(h.display, "AT&T (company)");
        assert_eq!(h.keys(), vec!["at&t (company)"]);
    }
}
