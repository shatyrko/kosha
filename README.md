# Kosha

A modern dictionary app for Linux and macOS that reads your dictionaries **where they are**:
ABBYY Lingvo DSL files (plain `.dsl` or compressed `.dsl.dz`) are opened in place, never
converted or copied. Kosha scans a file once to index its headwords, keeps that small index in
its cache folder, and reads an article straight from the original file when you look a word up.

Status: early, but usable: the app window, the engine and the command-line tool work on Linux;
the macOS build is set up but not tried yet.

## The app

- Search on the left, the articles of every dictionary on the right; the top word's articles show
  while you type, ↑/↓ move through the list, Enter keeps a word in the history.
- Cross-references open the word they point to; Back / Forward (Alt+←/→, ⌘[ / ⌘]).
- Sounds play from the dictionary, pictures are shown; labels (`n`, `v`, …) explain themselves on
  hover; secondary text can be hidden; Ctrl+= / Ctrl+− change the text size.
- Dictionaries: folders to look in, each dictionary on or off, the order articles appear in.
- Light and dark, following the system; one column in a narrow window.
- `kosha-app WORD` opens on that word; with Kosha already open, the word goes to that window.
- Settings (Ctrl+,): appearance (system, light, dark), text size, secondary text, playing the
  pronunciation, dictionaries, study defaults, where your data is, the keyboard shortcuts.

## Word cards

For learning a language: **Save** on an article (or Ctrl+S, into the folder used last) keeps the
word as a card, with a copy of its article, so it stays readable even if the dictionary goes.

- Folders in a tree, as deep as you like: create, rename, nest, move (drag a folder onto another),
  delete (keeping the cards or with them). Drag cards onto a folder to move them.
- Each card: the article, your own note, its folder, "I know it", Look up again.
- **Study**: the cards of a folder one at a time, large. Space flips, ← → move, 1 "Again",
  2 "Got it" (counted on the card). Word first or meaning first (the word, its transcription and
  sound hidden, so you recall it), shuffle, only the cards not known yet; at the end, repeat
  just the ones you missed.
- Saved in one file, `cards.json` in Kosha's data folder (`~/.local/share/kosha` on Linux,
  `~/Library/Application Support/kosha` on macOS), written safely after every change; if it
  can't be read, Kosha never writes over it.

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

Rust 1.80 or newer; for the app also Node.js (only for the Tauri command-line tool) and, on
Linux, WebKitGTK 4.1 (`webkit2gtk-4.1`).

```
cargo build --release -p kosha-cli   # target/release/kosha
npm install && npx tauri build       # the app and its packages (deb, rpm, AppImage / app, dmg)
cargo build --release -p kosha-app   # or just the app's program: target/release/kosha-app
cargo test
python3 tools/make-samples.py        # rewrites the sample dictionaries in samples/
```

The window is plain HTML, CSS and JavaScript modules in `ui/` (no framework, no build step); it
is built into the program, so rebuild after changing it.

On Linux, `bash tools/install-linux.sh` builds both and installs them for your user only
(`~/.local/bin/kosha-app`, `~/.local/bin/kosha`, a launcher entry and the icon; `--remove` takes
them out again).

`samples/` has two small dictionaries made up for Kosha (English–Russian in UTF-16 with
abbreviations, a sound and a picture; Russian–English in UTF-8, dictzip-compressed), free to use.

## License

GPL-3.0-or-later.
