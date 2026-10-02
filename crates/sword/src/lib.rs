//! Reader for CrossWire SWORD commentary modules (`zCom` / `zCom4`, ZIP-compressed),
//! keyed to the KJV versification.
//!
//! The format is implemented from its published description and verified empirically
//! against real modules; no SWORD code is used. See [`module`] for the on-disk layout and
//! what the reader promises.
//!
//! ```no_run
//! use kjv_sword::Module;
//! let module = Module::open_zip(std::path::Path::new("MHC.zip"))?;
//! for e in module.entries()? {
//!     println!("{} {}:{}-{}:{}", e.book, e.chapter, e.verse, e.to_chapter, e.to_verse);
//! }
//! # Ok::<(), kjv_sword::Error>(())
//! ```

pub mod conf;
pub mod error;
pub mod kjv;
pub mod module;

pub use conf::Conf;
pub use error::Error;
pub use kjv::{Slot, Testament};
pub use module::{
    BlockType, Driver, Encoding, Entry, Extraction, Heading, HeadingKind, Module, Orphan, Repeat,
    TestamentReport,
};
