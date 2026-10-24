//! Kosha's dictionary engine.
//!
//! Dictionaries are read where they are: a DSL file (plain or dictzip `.dz`) is scanned once to
//! build an index of its headwords and article positions, saved in the cache folder; looking a word
//! up reads just that article from the original file. Nothing is converted or copied.

pub mod appledict;
pub mod cards;
pub mod dictionary;
pub mod dsl;
pub mod headword;
pub mod index;
pub mod library;
pub mod normalize;
pub mod render;
pub mod source;
pub mod text;

pub use dictionary::{Article, DictInfo, Dictionary};
pub use library::{Hit, Library, Settings};
