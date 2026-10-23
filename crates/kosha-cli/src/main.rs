//! `kosha`: the dictionaries from the command line (and for the launcher and shortcuts).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use kosha_core::library::{self, find_files, Library, Settings};

const HELP: &str = "\
kosha - look words up in your dictionaries (ABBYY Lingvo DSL, plain or .dz)

usage:
  kosha lookup WORD        every dictionary's article, as text
  kosha search TEXT        headwords starting with TEXT (similar ones if few)
  kosha dicts              the dictionaries found, with their size
  kosha folders            the folders searched for dictionaries
  kosha folders add DIR    search DIR (and its sub-folders) too
  kosha folders remove DIR
  kosha index              (re)build every index now and show how long it took

options:
  --json                   machine-readable output (lookup, search, dicts)
  --html                   lookup: the articles as HTML
  --limit N                search: at most N headwords (default 20)
  --folder DIR             use DIR instead of the saved folders (repeatable)
  --no-cache               don't read or write saved indexes
";

struct Opts {
    json: bool,
    html: bool,
    limit: usize,
    folders: Vec<PathBuf>,
    no_cache: bool,
    args: Vec<String>,
}

fn parse() -> Result<Opts, String> {
    let mut o = Opts {
        json: false,
        html: false,
        limit: 20,
        folders: Vec::new(),
        no_cache: false,
        args: Vec::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => o.json = true,
            "--html" => o.html = true,
            "--no-cache" => o.no_cache = true,
            "--limit" => {
                o.limit = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--limit needs a number")?
            }
            "--folder" => o
                .folders
                .push(it.next().ok_or("--folder needs a folder")?.into()),
            "-h" | "--help" => o.args.insert(0, "help".into()),
            _ => o.args.push(a),
        }
    }
    Ok(o)
}

fn open(o: &Opts) -> Library {
    let mut settings = Settings::load();
    if !o.folders.is_empty() {
        settings.folders = o.folders.clone();
    }
    let cache = library::cache_dir();
    let lib = Library::open(
        &find_files(&settings.folders),
        &settings,
        (!o.no_cache).then_some(cache.as_path()),
    );
    for f in &lib.failed {
        eprintln!("kosha: can't read {}: {}", f.path, f.error);
    }
    lib
}

fn run() -> Result<(), String> {
    let o = parse()?;
    let cmd = o.args.first().map(String::as_str).unwrap_or("help");
    let rest = o.args.get(1..).unwrap_or_default().join(" ");
    match cmd {
        "lookup" | "l" => {
            if rest.trim().is_empty() {
                return Err("lookup: which word?".into());
            }
            let lib = open(&o);
            let arts = lib.lookup(&rest, &|_, name| name.to_string());
            if o.json {
                println!(
                    "{}",
                    serde_json::to_string(&arts).map_err(|e| e.to_string())?
                );
            } else if arts.is_empty() {
                let hits = lib.search(&rest, 5);
                let alt: Vec<_> = hits.iter().map(|h| h.word.as_str()).collect();
                if alt.is_empty() {
                    return Err(format!("\"{rest}\": not found"));
                }
                return Err(format!(
                    "\"{rest}\": not found. Did you mean: {}",
                    alt.join(", ")
                ));
            } else {
                for (i, a) in arts.iter().enumerate() {
                    if i > 0 {
                        println!();
                    }
                    println!("── {} · {}", a.headwords.join(", "), a.dict_name);
                    println!("{}", if o.html { &a.html } else { &a.text });
                }
            }
        }
        "search" | "s" => {
            let lib = open(&o);
            let hits = lib.search(&rest, o.limit);
            if o.json {
                println!(
                    "{}",
                    serde_json::to_string(&hits).map_err(|e| e.to_string())?
                );
            } else {
                for h in hits {
                    println!("{}{}", h.word, if h.fuzzy { "  (similar)" } else { "" });
                }
            }
        }
        "dicts" => {
            let lib = open(&o);
            if o.json {
                let v: Vec<_> = lib
                    .infos()
                    .into_iter()
                    .map(|(i, on)| serde_json::json!({ "enabled": on, "info": i }))
                    .collect();
                println!("{}", serde_json::Value::Array(v));
            } else {
                for (i, on) in lib.infos() {
                    let langs =
                        [i.index_language.as_str(), i.contents_language.as_str()].join(" → ");
                    println!(
                        "{} {}  ({}; {} articles, {} headword forms{}{})\n    {}",
                        if on { "●" } else { "○" },
                        i.name,
                        langs,
                        i.articles,
                        i.words,
                        if i.abbreviations > 0 {
                            format!(", {} abbreviations", i.abbreviations)
                        } else {
                            String::new()
                        },
                        if i.has_resources {
                            ", sounds/pictures"
                        } else {
                            ""
                        },
                        i.path
                    );
                }
            }
        }
        "folders" => {
            let mut s = Settings::load();
            let sub = o.args.get(1).map(String::as_str);
            let dir = o.args.get(2..).unwrap_or_default().join(" ");
            match sub {
                None => {
                    if s.folders.is_empty() {
                        println!("(none yet: kosha folders add DIR)");
                    }
                    for f in &s.folders {
                        println!("{}", f.display());
                    }
                }
                Some("add") => {
                    let p = PathBuf::from(&dir)
                        .canonicalize()
                        .map_err(|e| format!("{dir}: {e}"))?;
                    if !s.folders.contains(&p) {
                        s.folders.push(p);
                    }
                    s.save().map_err(|e| e.to_string())?;
                }
                Some("remove") => {
                    let p = PathBuf::from(&dir);
                    let c = p.canonicalize().unwrap_or(p);
                    s.folders.retain(|f| *f != c);
                    s.save().map_err(|e| e.to_string())?;
                }
                Some(x) => return Err(format!("folders: unknown command {x}")),
            }
        }
        "index" => {
            let t = Instant::now();
            let lib = Library::open(
                &find_files(&if o.folders.is_empty() {
                    Settings::load().folders
                } else {
                    o.folders.clone()
                }),
                &Settings::load(),
                None,
            );
            let words: usize = lib.dicts.iter().map(|d| d.index.words.len()).sum();
            let cache = library::cache_dir();
            for d in &lib.dicts {
                d.index
                    .save(&cache.join(format!("{}.idx", d.id)))
                    .map_err(|e| e.to_string())?;
            }
            println!(
                "{} dictionaries, {} headword forms, {:.2?}",
                lib.dicts.len(),
                words,
                t.elapsed()
            );
        }
        "help" => print!("{HELP}"),
        other => return Err(format!("unknown command {other} (kosha --help)")),
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kosha: {e}");
            ExitCode::FAILURE
        }
    }
}
