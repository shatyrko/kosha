//! Sanskrit verse analysis, the parts that are computed (no AI): transliteration, the meter, and
//! word-by-word dictionary lookup.
//!
//! - Transliteration: `vidyut-lipi` (Devanāgarī, IAST, Harvard-Kyoto, ITRANS, SLP1, … detected).
//! - Meter: `vidyut-chandas` scans syllables (light / heavy) and knows the fixed meters (vṛtta) of
//!   the Vṛttaratnākara and the mātrā meters (jāti: āryā, …); the anuṣṭubh śloka, the commonest
//!   meter, is checked here by its rules.
//! - Words: each word is looked up in the Sanskrit dictionaries (headword in Devanāgarī, exact);
//!   when it is not there, common case endings are taken off, and a compound is tried as two words.
//!   These are guesses at the dictionary form, marked as such, not a grammatical analysis.

use serde::{Deserialize, Serialize};
use vidyut_chandas::{Akshara, Chandas, MatchType, Weight};
use vidyut_lipi::{detect, Lipika, Scheme};

use crate::library::Library;
use crate::normalize;

const METERS: &str = include_str!("../data/meters.tsv");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Syllable {
    /// in IAST
    pub text: String,
    pub heavy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meter {
    /// e.g. "anuṣṭubh (śloka)", "vasantatilakā"
    pub name: String,
    /// how well it fits, in words
    pub note: String,
    /// the syllables of every pāda
    pub padas: Vec<Vec<Syllable>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meaning {
    pub dict_name: String,
    pub dict_id: String,
    /// the start of the article, as text
    pub gloss: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    /// the dictionary form looked up, in IAST and Devanāgarī
    pub iast: String,
    pub deva: String,
    pub meanings: Vec<Meaning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordRow {
    /// as written in the verse (IAST)
    pub word: String,
    pub deva: String,
    /// how it was found: "as written", "dictionary form", "split" (joined words or a compound);
    /// empty: not found
    pub how: String,
    /// one part, or the members of a compound
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Analysis {
    /// the input's script, as detected
    pub scheme: String,
    /// lines of the verse (speaker lines like "śrībhagavān uvāca" included)
    pub deva: Vec<String>,
    pub iast: Vec<String>,
    pub meter: Option<Meter>,
    pub words: Vec<WordRow>,
}

fn scheme_name(s: Scheme) -> &'static str {
    match s {
        Scheme::Devanagari => "Devanāgarī",
        Scheme::Iast => "IAST",
        Scheme::Iso15919 => "ISO 15919",
        Scheme::HarvardKyoto => "Harvard-Kyoto",
        Scheme::Itrans => "ITRANS",
        Scheme::Slp1 => "SLP1",
        Scheme::Velthuis => "Velthuis",
        Scheme::Wx => "WX",
        _ => "other",
    }
}

/// The verse's lines: split at line ends and daṇḍas, verse numbers dropped.
fn lines_of(text: &str) -> Vec<String> {
    text.split(['\n', '|', '।', '॥'])
        .map(|l| {
            l.chars()
                .filter(|c| !c.is_ascii_digit() && !('\u{0966}'..='\u{096F}').contains(c))
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|l| !l.is_empty())
        .collect()
}

/// A speaker line ("śrībhagavān uvāca", "arjuna uvāca"): not part of the meter.
fn is_speaker(slp1: &str) -> bool {
    slp1.trim_end().ends_with("uvAca")
}

pub struct Analyzer {
    chandas: Chandas,
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl Analyzer {
    pub fn new() -> Analyzer {
        // the table is part of Kosha; a malformed line would be a build mistake, not user input
        let chandas = Chandas::from_text(METERS).unwrap_or_else(|_| Chandas::new(Vec::new()));
        Analyzer { chandas }
    }

    pub fn analyze(&self, text: &str, lib: Option<&Library>) -> Analysis {
        let from = detect(text).unwrap_or(Scheme::Iast);
        let mut lipika = Lipika::new();
        let lines = lines_of(text);
        let slp1: Vec<String> = lines
            .iter()
            .map(|l| lipika.transliterate(l, from, Scheme::Slp1))
            .collect();
        let deva = slp1
            .iter()
            .map(|l| lipika.transliterate(l, Scheme::Slp1, Scheme::Devanagari))
            .collect();
        let iast: Vec<String> = slp1
            .iter()
            .map(|l| lipika.transliterate(l, Scheme::Slp1, Scheme::Iast))
            .collect();
        let verse: Vec<&String> = slp1.iter().filter(|l| !is_speaker(l)).collect();
        let meter = self.meter(&verse, &mut lipika);
        let words = match lib {
            Some(lib) => slp1
                .iter()
                .flat_map(|l| l.split_whitespace())
                .map(|w| word_row(w, lib, &mut lipika))
                .collect(),
            None => Vec::new(),
        };
        Analysis {
            scheme: scheme_name(from).into(),
            deva,
            iast,
            meter,
            words,
        }
    }

    fn meter(&self, verse: &[&String], lipika: &mut Lipika) -> Option<Meter> {
        if verse.is_empty() {
            return None;
        }
        let text = verse
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let found = self.chandas.classify(&text);
        let all: Vec<&Akshara> = found.aksharas().iter().flatten().collect();
        let to_syl = |a: &Akshara, lipika: &mut Lipika| Syllable {
            text: lipika.transliterate(a.text(), Scheme::Slp1, Scheme::Iast),
            heavy: a.weight() == Weight::G,
        };
        if let (Some(v), MatchType::Full | MatchType::Pada) = (found.padya(), found.match_type()) {
            let padas = found
                .aksharas()
                .iter()
                .map(|line| line.iter().map(|a| to_syl(a, lipika)).collect())
                .collect();
            let note = if found.match_type() == MatchType::Full {
                "every syllable fits"
            } else {
                "fits pāda by pāda"
            };
            return Some(Meter {
                name: lipika.transliterate(v.name(), Scheme::Slp1, Scheme::Iast),
                note: note.into(),
                padas,
            });
        }
        // anuṣṭubh śloka: 4 pādas of 8 syllables
        if all.len() == 32 {
            let padas: Vec<Vec<Syllable>> = all
                .chunks(8)
                .map(|c| c.iter().map(|a| to_syl(a, lipika)).collect())
                .collect();
            let (ok, note) = sloka(&padas);
            if ok {
                return Some(Meter {
                    name: "anuṣṭubh (śloka)".into(),
                    note,
                    padas,
                });
            }
            return Some(Meter {
                name: "anuṣṭubh?".into(),
                note,
                padas,
            });
        }
        let padas = found
            .aksharas()
            .iter()
            .map(|line| line.iter().map(|a| to_syl(a, lipika)).collect())
            .collect();
        Some(Meter {
            name: "not recognised".into(),
            note: format!("{} syllables; a śloka has 32 (4 × 8)", all.len()),
            padas,
        })
    }
}

/// The śloka's rules, pāda by pāda (positions 1–8): the 5th syllable light, the 6th heavy; the 7th
/// heavy in odd pādas (1, 3) and light in even ones (2, 4) — the "pathyā" form. Odd pādas may
/// instead take one of the accepted variants (vipulā), named by their 5th–7th syllables.
fn sloka(padas: &[Vec<Syllable>]) -> (bool, String) {
    let w = |p: &Vec<Syllable>, i: usize| p[i - 1].heavy;
    let mut notes = Vec::new();
    let mut ok = true;
    for (n, p) in padas.iter().enumerate() {
        let odd = n % 2 == 0;
        let pathya = !w(p, 5) && w(p, 6) && (w(p, 7) == odd);
        if pathya {
            continue;
        }
        if odd {
            let pattern = (w(p, 5), w(p, 6), w(p, 7));
            let name = match pattern {
                (false, false, false) => Some("na-vipulā"),
                (true, false, false) => Some("bha-vipulā"),
                (true, true, true) => Some("ma-vipulā"),
                (true, false, true) => Some("ra-vipulā"),
                _ => None,
            };
            match name {
                Some(v) => notes.push(format!("pāda {}: {v}", n + 1)),
                None => {
                    ok = false;
                    notes.push(format!("pāda {}: syllables 5–7 do not fit", n + 1));
                }
            }
        } else {
            ok = false;
            notes.push(format!(
                "pāda {}: syllables 5–7 should be light–heavy–light",
                n + 1
            ));
        }
    }
    let note = if notes.is_empty() {
        "4 pādas of 8 syllables, every pāda in the regular (pathyā) form".to_string()
    } else {
        format!("4 pādas of 8 syllables; {}", notes.join("; "))
    };
    (ok, note)
}

/// Endings taken off to find a dictionary form (SLP1, longest first): (ending, stem ending).
const ENDINGS: &[(&str, &str)] = &[
    ("AnAm", "a"),
    ("eByaH", "a"),
    ("ayoH", "a"),
    ("asya", "a"),
    ("Ena", "a"),
    ("ena", "a"),
    ("Aya", "a"),
    ("ezu", "a"),
    ("Ani", "a"),
    ("EH", "a"),
    ("At", "a"),
    ("AH", "a"),
    ("An", "a"),
    ("am", "a"),
    ("aM", "a"),
    ("aH", "a"),
    ("as", "a"),
    ("o", "a"),
    ("e", "a"),
    ("O", "a"),
    ("AyAH", "A"),
    ("ayA", "A"),
    ("AyAm", "A"),
    ("Am", "A"),
    ("AM", "A"),
    ("vAn", "vat"),
    ("mAn", "mat"),
    ("ayaH", "i"),
    ("inA", "i"),
    ("iH", "i"),
    ("im", "i"),
    ("iM", "i"),
    ("avaH", "u"),
    ("unA", "u"),
    ("uH", "u"),
    ("um", "u"),
    ("uM", "u"),
    ("A", "a"),
    ("A", "an"),
    ("I", "in"),
    ("am", ""),
    ("m", ""),
    ("M", ""),
    ("H", ""),
];

fn sanskrit_dicts(lib: &Library) -> impl Iterator<Item = &crate::Dictionary> {
    lib.dicts
        .iter()
        .filter(|d| d.index.header.index_language == "Sanskrit")
}

fn known(lib: &Library, deva: &str) -> bool {
    let key = normalize::key(deva);
    sanskrit_dicts(lib).any(|d| !d.index.exact(&key).is_empty())
}

/// Dictionary forms to try for a word (SLP1), the word itself first.
fn candidates(word: &str) -> Vec<String> {
    let mut out = vec![word.to_string()];
    // a final s / r / o is often a visarga changed by sandhi
    if let Some(stem) = word.strip_suffix(['s', 'r']) {
        out.push(format!("{stem}H"));
    }
    if let Some(stem) = word.strip_suffix('o') {
        out.push(format!("{stem}aH"));
    }
    for (end, repl) in ENDINGS {
        if let Some(stem) = word.strip_suffix(end) {
            if stem.chars().count() >= 2 {
                out.push(format!("{stem}{repl}"));
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(c.clone()));
    out
}

/// What the Cologne dictionaries add for their editors, not for readers: page references,
/// "PDF" and "Correction" links, correction notes.
fn clean_gloss(text: &str) -> String {
    // Whitney's roots have line breaks written out as "\n"
    let mut t = text.replace("\\n", " ");
    for junk in ["PDF Correction", " PDF", " Correction"] {
        while let Some(i) = t.rfind(junk) {
            if i + junk.len() == t.trim_end().len() {
                t.truncate(i);
            } else {
                break;
            }
        }
    }
    // [Page0390-a+ 59], {c9c}: editorial references
    let mut out = String::with_capacity(t.len());
    let mut skip: Option<char> = None;
    let mut rest = t.as_str();
    while let Some(c) = rest.chars().next() {
        if skip.is_none()
            && (rest.starts_with("[Page")
                || (c == '{'
                    && rest[1..]
                        .chars()
                        .next()
                        .is_some_and(|n| n.is_ascii_alphanumeric())))
        {
            skip = Some(if c == '[' { ']' } else { '}' });
        } else if Some(c) == skip {
            skip = None;
        } else if skip.is_none() {
            out.push(c);
        }
        rest = &rest[c.len_utf8()..];
    }
    let out = match out.find("Correction submitted") {
        Some(i) => out[..i].to_string(),
        None => out,
    };
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The entries for a dictionary form: up to three homonyms from every Sanskrit dictionary, in
/// the dictionaries' order.
fn meanings(lib: &Library, deva: &str) -> Vec<Meaning> {
    let key = normalize::key(deva);
    let mut out = Vec::new();
    for d in sanskrit_dicts(lib) {
        let mut seen = std::collections::HashSet::new();
        for w in d
            .index
            .exact(&key)
            .iter()
            .filter(|w| seen.insert(w.article))
            .take(3)
        {
            let Ok(a) = d.article(w.article, &|_, n| n.to_string()) else {
                continue;
            };
            let text = a.text.split_whitespace().collect::<Vec<_>>().join(" ");
            // the article usually starts with its headword: show what follows
            let text = text
                .strip_prefix(deva)
                .unwrap_or(&text)
                .trim_start_matches([' ', ',', '.'])
                .to_string();
            let text = clean_gloss(&text);
            if text.is_empty() {
                continue;
            }
            let gloss: String = text.chars().take(200).collect();
            let gloss = if text.chars().count() > 200 {
                format!("{}…", gloss.trim_end())
            } else {
                gloss
            };
            out.push(Meaning {
                dict_name: a.dict_name,
                dict_id: a.dict_id,
                gloss,
            });
        }
    }
    out
}

fn part(lib: &Library, slp1: &str, lipika: &mut Lipika) -> Part {
    let deva = lipika.transliterate(slp1, Scheme::Slp1, Scheme::Devanagari);
    Part {
        iast: lipika.transliterate(slp1, Scheme::Slp1, Scheme::Iast),
        meanings: meanings(lib, &deva),
        deva,
    }
}

/// The fewest pieces (at least two, each a dictionary word, possibly with an ending) that make up
/// `word` exactly; ties go to the split with more pieces found as written. Sandhi changes at the
/// joins (vowels merging) are not undone here: that needs the grammatical analysis of stage 2.
fn segment(lib: &Library, word: &str, lipika: &mut Lipika) -> Option<Vec<String>> {
    const MAX_PIECE: usize = 24;
    let chars: Vec<char> = word.chars().collect();
    let n = chars.len();
    if n < 4 {
        return None;
    }
    let mut cache: std::collections::HashMap<(usize, usize), Option<(String, bool)>> =
        Default::default();
    // best[i]: (pieces, pieces found as written, the pieces) for chars[..i]
    let mut best: Vec<Option<(usize, usize, Vec<String>)>> = vec![None; n + 1];
    best[0] = Some((0, 0, Vec::new()));
    for i in 0..n {
        let Some((count, exact, pieces)) = best[i].clone() else {
            continue;
        };
        for j in (i + 2)..=n.min(i + MAX_PIECE) {
            if i == 0 && j == n {
                continue; // the whole word was tried already
            }
            let found = cache
                .entry((i, j))
                .or_insert_with(|| find(lib, &chars[i..j].iter().collect::<String>(), lipika))
                .clone();
            let Some((form, as_written)) = found else {
                continue;
            };
            let cand = (count + 1, exact + usize::from(as_written));
            let better = match &best[j] {
                None => true,
                Some((c, e, _)) => cand.0 < *c || (cand.0 == *c && cand.1 > *e),
            };
            if better {
                let mut p = pieces.clone();
                p.push(form);
                best[j] = Some((cand.0, cand.1, p));
            }
        }
    }
    best[n].take().map(|(_, _, p)| p).filter(|p| p.len() >= 2)
}

/// The first dictionary form of `word` the dictionaries know.
fn find(lib: &Library, word: &str, lipika: &mut Lipika) -> Option<(String, bool)> {
    for (i, c) in candidates(word).into_iter().enumerate() {
        let deva = lipika.transliterate(&c, Scheme::Slp1, Scheme::Devanagari);
        if known(lib, &deva) {
            return Some((c, i == 0));
        }
    }
    None
}

fn word_row(word: &str, lib: &Library, lipika: &mut Lipika) -> WordRow {
    let clean: String = word
        .chars()
        .filter(|c| !matches!(c, '-' | ',' | ';' | '.' | '\'' | '’'))
        .collect();
    let mut row = WordRow {
        word: lipika.transliterate(word, Scheme::Slp1, Scheme::Iast),
        deva: lipika.transliterate(word, Scheme::Slp1, Scheme::Devanagari),
        how: String::new(),
        parts: Vec::new(),
    };
    if let Some((form, exact)) = find(lib, &clean, lipika) {
        row.how = if exact {
            "as written".into()
        } else {
            "dictionary form".into()
        };
        row.parts.push(part(lib, &form, lipika));
        return row;
    }
    // joined words (sandhi in the text, or a compound): the fewest dictionary words that make it up
    if let Some(pieces) = segment(lib, &clean, lipika) {
        row.how = "split".into();
        for form in pieces {
            row.parts.push(part(lib, &form, lipika));
        }
        return row;
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    const GITA_2_2: &str =
        "श्रीभगवानुवाच ।\nकुतस्त्वा कश्मलमिदं विषमे समुपस्थितम् ।\nअनार्यजुष्टमस्वर्ग्यमकीर्तिकरमर्जुन ॥ २ ॥";

    #[test]
    fn transliteration_and_sloka() {
        let a = Analyzer::new().analyze(GITA_2_2, None);
        assert_eq!(a.scheme, "Devanāgarī");
        assert_eq!(a.iast[1], "kutastvā kaśmalamidaṃ viṣame samupasthitam");
        let m = a.meter.unwrap();
        assert_eq!(m.name, "anuṣṭubh (śloka)", "{}", m.note);
        assert_eq!(m.padas.len(), 4);
        assert!(m.padas.iter().all(|p| p.len() == 8));
    }

    #[test]
    fn iast_input_too() {
        let a = Analyzer::new().analyze("kutas tvā kaśmalam idaṃ viṣame samupasthitam", None);
        assert_eq!(a.scheme, "IAST");
        assert_eq!(a.deva[0], "कुतस् त्वा कश्मलम् इदं विषमे समुपस्थितम्");
    }

    #[test]
    fn glosses_without_editorial_notes() {
        assert_eq!(
            clean_gloss(
                "a. [Uṇ. 1. 106.] Foul, dirty; [Page0390-a+ 59] disgraceful PDF Correction"
            ),
            "a. [Uṇ. 1. 106.] Foul, dirty; disgraceful"
        );
        assert_eq!(
            clean_gloss("{c9c} U. To cook. Correction submitted by x on 2017"),
            "U. To cook."
        );
        assert_eq!(
            clean_gloss("√vac 'speak' \\npresent: vakti"),
            "√vac 'speak' present: vakti"
        );
    }

    #[test]
    fn endings() {
        let c = candidates("kaSmalam");
        assert!(c.contains(&"kaSmala".to_string()));
        assert!(candidates("viSame").contains(&"viSama".to_string()));
        assert!(candidates("kutas").contains(&"kutaH".to_string()));
        assert!(candidates("BagavAn").contains(&"Bagavat".to_string()));
    }
}
