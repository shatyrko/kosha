//! Search keys: what a headword or a query is compared as.
//!
//! Lower case, accents and other combining marks removed (so "ё" finds "е", "Café" finds "cafe"),
//! runs of white space collapsed to one space, and a few look-alike characters folded together
//! (typographic apostrophes and dashes). Applied the same way to headwords when indexing and to
//! what the user types.

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

pub fn key(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.nfd() {
        if is_combining_mark(c) {
            continue;
        }
        let c = match c {
            '\u{2019}' | '\u{2018}' | '\u{02BC}' | '`' | '\u{00B4}' => '\'',
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            c => c,
        };
        if c.is_whitespace() {
            space = !out.is_empty();
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        for l in c.to_lowercase() {
            out.push(l);
        }
    }
    out
}

/// Edit distance with an upper bound: `None` as soon as it is known to exceed `max`.
pub fn bounded_distance(a: &[char], b: &[char], max: usize) -> Option<usize> {
    if a.len().abs_diff(b.len()) > max {
        return None;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        let mut row_min = cur[0];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
            row_min = row_min.min(cur[j + 1]);
        }
        if row_min > max {
            return None;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let d = prev[b.len()];
    (d <= max).then_some(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_case_accents_space() {
        assert_eq!(key("  Café   au  Lait "), "cafe au lait");
        assert_eq!(key("Ёлка"), "елка");
        assert_eq!(key("rock’n’roll"), "rock'n'roll");
        assert_eq!(key("Straße"), "straße");
    }

    #[test]
    fn distance() {
        let c = |s: &str| s.chars().collect::<Vec<_>>();
        assert_eq!(bounded_distance(&c("kitten"), &c("sitting"), 3), Some(3));
        assert_eq!(bounded_distance(&c("kitten"), &c("sitting"), 2), None);
        assert_eq!(bounded_distance(&c("word"), &c("word"), 0), Some(0));
    }
}
