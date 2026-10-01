//! Original language support for Hebrew OT and Greek NT
//!
//! This module provides parsing and loading of STEP Bible data files
//! which include Hebrew/Greek texts with Strong's numbers and morphology.

pub mod loader;

pub use loader::load_extended_bible;
