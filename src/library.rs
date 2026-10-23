//! Dimensions and alphabet of the Library.
//!
//! Borges (1941): each hexagon has four walls of books, each wall five
//! shelves, each shelf thirty-two books, each book 410 pages, each page 40
//! lines, each line 80 symbols. The hexagon number is bounded here at
//! [`HEXAGON_BYTES`] bytes.

/// Walls bearing books in each hexagon.
pub const WALLS_PER_HEXAGON: u32 = 4;
/// Shelves per wall.
pub const SHELVES_PER_WALL: u32 = 5;
/// Books per shelf.
pub const BOOKS_PER_SHELF: u32 = 32;
/// Pages per book.
pub const PAGES_PER_BOOK: u32 = 410;
/// Lines per page.
pub const LINES_PER_PAGE: usize = 40;
/// Symbols per line.
pub const CHARS_PER_LINE: usize = 80;
/// Width of a hexagon number in bytes.
pub const HEXAGON_BYTES: usize = 64;

/// The blank symbol.
pub const BLANK: u8 = b' ';

/// The alphabet, for error messages.
pub const ALPHABET_DESCRIPTION: &str = "a-z, space, comma, period";

/// Whether `c` belongs to the 29-symbol alphabet.
///
/// ```
/// use vibabel::library::is_allowed;
/// assert!(is_allowed('q') && is_allowed(','));
/// assert!(!is_allowed('Q') && !is_allowed('!'));
/// ```
pub fn is_allowed(c: char) -> bool {
    c.is_ascii_lowercase() || matches!(c, ' ' | ',' | '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alphabet_has_29_symbols() {
        let n = (0..=255u8).filter(|&b| is_allowed(b.into())).count();
        assert_eq!(n, 29);
        assert!(is_allowed(BLANK.into()));
    }
}
