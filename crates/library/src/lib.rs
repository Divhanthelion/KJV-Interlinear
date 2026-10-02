//! The study library: Bible translations, commentaries, and cross-references.
//! See docs/LIBRARY.md.

pub mod archive;
pub mod books;
pub mod library;
pub mod reference;
pub mod usfm;
pub mod view;

pub use library::{BibleInfo, BookEntry, Library};
