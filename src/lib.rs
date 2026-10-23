//! A vi-like editor for pages of the Library of Babel.
//!
//! - [`library`]: dimensions and alphabet.
//! - [`address`]: page addresses and page IDs.
//! - [`page`]: the 40 × 80 page and its file format.
//! - [`editor`]: editing logic.
//! - [`terminal`]: rendering and input.

pub mod address;
pub mod editor;
pub mod library;
pub mod page;
pub mod terminal;
