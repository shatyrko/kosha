//! Speed check on a big generated dictionary: `cargo run --release --example bigdict -- DIR [ARTICLES]`
//! writes DIR/big.dsl (UTF-16, like Lingvo) and DIR/big-dz.dsl.dz, then times indexing, loading the
//! saved index, searching and looking up.

use std::path::PathBuf;
use std::time::Instant;

use kosha_core::library::{find_files, Library, Settings};
use kosha_core::source::write_dictzip;
use kosha_core::text::encode_utf16le_with_bom;

fn word(mut n: usize) -> String {
    const SYL: [&str; 16] = [
        "ka", "lo", "mi", "ne", "ru", "sa", "to", "vi", "bre", "cho", "dra", "fle", "gri", "pla",
        "stu", "zo",
    ];
    let mut s = String::new();
    loop {
        s.push_str(SYL[n % 16]);
        n /= 16;
        if n == 0 {
            break s;
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("DIR"));
    let n: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(300_000);
    std::fs::create_dir_all(&dir).unwrap();
    let mut text = String::from(
        "#NAME \"Big generated\"\n#INDEX_LANGUAGE \"English\"\n#CONTENTS_LANGUAGE \"Russian\"\n\n",
    );
    for i in 0..n {
        let w = word(i);
        text.push_str(&format!(
            "{w}\n{w} (up)\n\t[m0][b]{w}[/b] [t]ˈ{w}[/t] [s]{w}.wav[/s]\n\t[m1][p]n[/p] [trn]перевод номер {i}[/trn]\n\t[m2][ex]an ~ example — пример с {w}[/ex]\n\t[m2][*][com]comment[/com][/*] see <<{}>>\n",
            word(i + 1)
        ));
    }
    let bytes = encode_utf16le_with_bom(&text);
    std::fs::write(dir.join("big.dsl"), &bytes).unwrap();
    write_dictzip(&dir.join("big-dz.dsl.dz"), &bytes, 58315).unwrap();
    println!(
        "{} articles, {:.1} MB of UTF-16",
        n,
        bytes.len() as f64 / 1e6
    );

    let files = find_files(std::slice::from_ref(&dir));
    let cache = dir.join("cache");
    let _ = std::fs::remove_dir_all(&cache);
    let s = Settings::default();
    let t = Instant::now();
    let lib = Library::open(&files, &s, Some(&cache));
    println!("index both (first time): {:.2?}", t.elapsed());
    drop(lib);
    let t = Instant::now();
    let lib = Library::open(&files, &s, Some(&cache));
    println!("open both (saved index): {:.2?}", t.elapsed());
    for q in ["sa", "stuzo", "brekalo", "plastu"] {
        let t = Instant::now();
        let hits = lib.search(q, 30);
        println!("search {q:>8}: {:>2} hits {:.2?}", hits.len(), t.elapsed());
    }
    let t = Instant::now();
    let hits = lib.search("brekalxo", 30);
    println!(
        "search typo brekalxo: {} hits ({} similar) {:.2?}",
        hits.len(),
        hits.iter().filter(|h| h.fuzzy).count(),
        t.elapsed()
    );
    let target = word(n / 2);
    let t = Instant::now();
    let arts = lib.lookup(&target, &|d, f| format!("kosha://{d}/{f}"));
    println!(
        "lookup {target}: {} articles {:.2?}",
        arts.len(),
        t.elapsed()
    );
    print!(
        "{}",
        arts.first().map(|a| a.text.clone()).unwrap_or_default()
    );
    println!();
}
