//! Render every article of every dictionary in the given folders and report any that fails or
//! panics: `cargo run --release --example sweep -- FOLDER...`
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::time::Instant;

use kosha_core::library::{find_files, Library, Settings};

fn main() {
    let folders: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let lib = Library::open(&find_files(&folders), &Settings::default(), None);
    let t = Instant::now();
    let (mut total, mut bad) = (0usize, 0usize);
    std::panic::set_hook(Box::new(|info| {
        eprintln!(
            "  at {}",
            info.location().map(|l| l.to_string()).unwrap_or_default()
        )
    }));
    for d in &lib.dicts {
        for i in 0..d.index.articles.len() as u32 {
            total += 1;
            match catch_unwind(AssertUnwindSafe(|| d.article(i, &|_, n| n.to_string()))) {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    bad += 1;
                    if bad <= 10 {
                        eprintln!("{} #{i}: {e}", d.name());
                    }
                }
                Err(p) => {
                    bad += 1;
                    if bad <= 10 {
                        let msg = p
                            .downcast_ref::<String>()
                            .cloned()
                            .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()));
                        eprintln!("{} #{i}: PANIC {}", d.name(), msg.unwrap_or_default());
                    }
                }
            }
        }
    }
    println!(
        "{} dictionaries, {total} articles rendered, {bad} failed, {:.1?}",
        lib.dicts.len(),
        t.elapsed()
    );
}
