use std::fmt;
use std::io;

use crate::kjv::Testament;

/// Everything that can go wrong reading a module. The reader never guesses: anything it
/// cannot prove about the data is an error, not a best effort.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Zip(zip::result::ZipError),
    /// The archive has no `mods.d/*.conf`, or more than one.
    Conf(String),
    /// A feature of the module this reader does not implement (driver, compression,
    /// versification, cipher, encoding, block type).
    Unsupported(String),
    /// A data file the module needs is not in the archive (or only some of a set are).
    MissingFile(String),
    /// The verse index has the wrong number of records for the KJV versification.
    SlotCount {
        testament: Testament,
        expected: usize,
        found: usize,
        file_len: usize,
        record_size: usize,
    },
    /// Structural damage: a block that will not decompress to its declared size, an
    /// entry outside its block, overlapping entries, and similar.
    Corrupt(String),
    /// Entry bytes that are not valid in the module's encoding.
    Decode(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Zip(e) => write!(f, "zip error: {e}"),
            Error::Conf(m) => write!(f, "module configuration: {m}"),
            Error::Unsupported(m) => write!(f, "unsupported: {m}"),
            Error::MissingFile(m) => write!(f, "missing file: {m}"),
            Error::SlotCount {
                testament,
                expected,
                found,
                file_len,
                record_size,
            } => write!(
                f,
                "{testament:?} verse index has {found} records of {record_size} bytes \
                 ({file_len} bytes); the KJV versification needs {expected}"
            ),
            Error::Corrupt(m) => write!(f, "corrupt module: {m}"),
            Error::Decode(m) => write!(f, "text decoding: {m}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Zip(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Error::Zip(e)
    }
}
