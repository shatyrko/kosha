# Kosha

A modern dictionary app for Linux and macOS that reads your dictionaries **where they are**:
ABBYY Lingvo DSL files (plain `.dsl` or compressed `.dsl.dz`) are opened in place, never
converted or copied. Kosha scans a file once to index its headwords, keeps that small index in
its cache folder, and reads an article straight from the original file when you look a word up.

Status: early. The engine and the command-line tool work; the app window (Tauri) is next.

## What it reads

- **DSL** (ABBYY Lingvo source format): UTF-16 LE/BE, UTF-8, and old Windows code pages
  (`#SOURCE_CODE_PAGE`); plain or dictzip `.dz` (only the chunk an article is in is decompressed).
- Headwords with unsorted parts `{...}`, optional parts `(...)` (searched with and without),
  several headwords per article, `@` sub-entries, `{{comments}}`, `\` escapes.
- Markup: bold, italics, colours, indents `[m0]`–`[m9]`, translations, examples, comments, labels
  `[p]` with their expansions from `<name>_abrv.dsl`, transcription, secondary text `[*]`,
  cross-references `[ref]` / `<<...>>`, links, `~` for the headword. Badly nested tags are repaired.
- Sounds and pictures `[s]...[/s]` from `<name>.dsl.files.zip`, `<name>.files.zip` or a
  `<name>.dsl.files` folder next to the dictionary.

Not supported: Lingvo's compiled `.lsd` files (convert them to `.dsl` first).

## Search

- As you type, across all dictionaries at once; the exact word first.
- Case and accents don't matter (`елка` finds `ёлка`, `cafe` finds `café`).
- Typos: when few words start with what you typed, similar words are offered (`aple` → `apple`).

On a 147 MB dictionary with 300,000 articles (two copies, plain and `.dz`): indexing both takes
1.7 s the first time, opening them from the saved index 0.13 s, a search about 0.05 ms, looking up
a word 0.2 ms.

## Command line

```
kosha folders add ~/Dictionaries     # where your dictionaries are (sub-folders included)
kosha dicts                          # what was found
kosha lookup apple                   # every dictionary's article, as text (--html, --json)
kosha search app                     # headwords starting with "app" (--json, --limit N)
```

## Building

Rust 1.80 or newer.

```
cargo build --release -p kosha-cli   # target/release/kosha
cargo test
python3 tools/make-samples.py        # rewrites the sample dictionaries in samples/
```

`samples/` has two small dictionaries made up for Kosha (English–Russian in UTF-16 with
abbreviations, a sound and a picture; Russian–English in UTF-8, dictzip-compressed), free to use.

## License

GPL-3.0-or-later.
