//! The page: a fixed grid of 40 × 80 symbols.
//!
//! A page has no length; every cell always holds a symbol, initially blank.
//!
//! # File format
//!
//! 40 lines of 80 symbols, each terminated by `\n` (3,240 bytes). On
//! reading, short lines and missing trailing lines are taken as blank.

use std::fs;
use std::io;
use std::path::Path;

use anyhow::{Context, Result, ensure};

use crate::library::{ALPHABET_DESCRIPTION, BLANK, CHARS_PER_LINE, LINES_PER_PAGE, is_allowed};

/// One page. Every cell holds a symbol of the alphabet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    lines: [[u8; CHARS_PER_LINE]; LINES_PER_PAGE],
}

impl Page {
    pub fn blank() -> Self {
        Self {
            lines: [[BLANK; CHARS_PER_LINE]; LINES_PER_PAGE],
        }
    }

    pub fn get(&self, line: usize, col: usize) -> char {
        self.lines[line][col].into()
    }

    /// Writes `c` at (`line`, `col`). Returns `false`, leaving the page
    /// unchanged, if `c` is outside the alphabet. This is the sole write
    /// path for symbols.
    pub fn set(&mut self, line: usize, col: usize, c: char) -> bool {
        let allowed = is_allowed(c);
        if allowed {
            self.lines[line][col] = c as u8;
        }
        allowed
    }

    /// The 80 symbols of `line`.
    pub fn line(&self, line: usize) -> &str {
        std::str::from_utf8(&self.lines[line]).expect("page is ASCII")
    }

    pub fn first_non_blank(&self, line: usize) -> Option<usize> {
        self.lines[line].iter().position(|&b| b != BLANK)
    }

    pub fn last_non_blank(&self, line: usize) -> Option<usize> {
        self.lines[line].iter().rposition(|&b| b != BLANK)
    }

    pub fn is_line_blank(&self, line: usize) -> bool {
        self.first_non_blank(line).is_none()
    }

    /// Removes columns `start..end` of `line`; the remainder shifts left and
    /// blanks fill the right margin.
    pub fn delete_chars(&mut self, line: usize, start: usize, end: usize) {
        let cells = &mut self.lines[line];
        cells.copy_within(end.., start);
        cells[CHARS_PER_LINE - (end - start)..].fill(BLANK);
    }

    /// Removes `line`; the lines below shift up and a blank line fills the
    /// bottom.
    pub fn delete_line(&mut self, line: usize) {
        self.lines.copy_within(line + 1.., line);
        self.lines[LINES_PER_PAGE - 1] = [BLANK; CHARS_PER_LINE];
    }

    /// Inserts a blank line at `line`, shifting the lines below down. The
    /// last line is discarded; callers ensure it is blank.
    pub fn open_line(&mut self, line: usize) {
        self.lines.copy_within(line..LINES_PER_PAGE - 1, line + 1);
        self.lines[line] = [BLANK; CHARS_PER_LINE];
    }

    pub fn to_text(&self) -> String {
        (0..LINES_PER_PAGE)
            .map(|line| format!("{}\n", self.line(line)))
            .collect()
    }

    pub fn from_text(text: &str) -> Result<Self> {
        let mut page = Self::blank();
        for (line, content) in text.lines().enumerate() {
            ensure!(line < LINES_PER_PAGE, "more than {LINES_PER_PAGE} lines");
            let n = line + 1;
            for (col, c) in content.chars().enumerate() {
                ensure!(
                    col < CHARS_PER_LINE,
                    "line {n}: more than {CHARS_PER_LINE} symbols"
                );
                ensure!(
                    page.set(line, col, c),
                    "line {n}: {c:?} is not in the alphabet ({ALPHABET_DESCRIPTION})"
                );
            }
        }
        Ok(page)
    }

    /// Reads the page at `path`. A missing file is a blank page.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => Self::from_text(&text).with_context(|| format!("{}", path.display())),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self::blank()),
            Err(err) => Err(err).with_context(|| format!("{}", path.display())),
        }
    }

    /// Writes the page to `path` atomically, via a temporary file and rename.
    pub fn save(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, self.to_text())
            .and_then(|()| fs::rename(&tmp, path))
            .with_context(|| format!("{}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(lines: &[&str]) -> Page {
        Page::from_text(&lines.join("\n")).unwrap()
    }

    #[test]
    fn set_rejects_foreign_symbols() {
        let mut p = Page::blank();
        assert!(p.set(0, 0, 'a'));
        for c in ['A', '!', '\t', 'é'] {
            assert!(!p.set(0, 1, c));
        }
        assert_eq!(p.line(0).trim_end(), "a");
    }

    #[test]
    fn text_round_trip() {
        let p = page(&["hello, world.", "", "the library is unlimited."]);
        let text = p.to_text();
        assert_eq!(text.len(), 3240);
        assert_eq!(Page::from_text(&text).unwrap(), p);
    }

    #[test]
    fn from_text_rejects_invalid() {
        assert!(Page::from_text("Hello").is_err());
        assert!(Page::from_text(&"a".repeat(81)).is_err());
        assert!(Page::from_text(&"a\n".repeat(41)).is_err());
    }

    #[test]
    fn delete_chars_shifts_left() {
        let mut p = page(&["abcdef"]);
        p.delete_chars(0, 1, 3);
        assert_eq!(p.line(0).trim_end(), "adef");
        let mut p = page(&[&"x".repeat(80)]);
        p.delete_chars(0, 79, 80);
        assert_eq!(p.line(0), format!("{} ", "x".repeat(79)));
    }

    #[test]
    fn delete_and_open_lines() {
        let mut p = page(&["one", "two", "three"]);
        p.delete_line(0);
        assert_eq!(p, page(&["two", "three"]));
        p.open_line(1);
        assert_eq!(p, page(&["two", "", "three"]));
    }

    #[test]
    fn save_and_load() {
        let dir = std::env::temp_dir().join(format!("vibabel-page-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("page");
        assert_eq!(Page::load(&path).unwrap(), Page::blank());
        let p = page(&["a page of the library."]);
        p.save(&path).unwrap();
        assert_eq!(Page::load(&path).unwrap(), p);
        fs::remove_dir_all(&dir).unwrap();
    }
}
