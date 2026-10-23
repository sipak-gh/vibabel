//! Editing logic, independent of any terminal.
//!
//! The editor is a state machine driven by [`Key`] events. Normal-mode
//! bindings and `:` commands are listed in the tables `NORMAL` and
//! `COMMANDS`, each entry pairing a key sequence or command name with its
//! action. Commands take no counts. The editing semantics are described in
//! `docs/DESIGN.md`.

use std::path::PathBuf;

use crate::address::Address;
use crate::library::{ALPHABET_DESCRIPTION, CHARS_PER_LINE, LINES_PER_PAGE, is_allowed};
use crate::page::Page;

const LAST_LINE: usize = LINES_PER_PAGE - 1;
const LAST_COL: usize = CHARS_PER_LINE - 1;
const UNDO_LIMIT: usize = 1000;

type Action = fn(&mut Editor);

/// Normal-mode bindings.
const NORMAL: &[(&str, Action)] = &[
    ("h", |e| e.cursor.col = e.cursor.col.saturating_sub(1)),
    ("l", |e| e.cursor.col = (e.cursor.col + 1).min(LAST_COL)),
    ("j", |e| e.cursor.line = (e.cursor.line + 1).min(LAST_LINE)),
    ("k", |e| e.cursor.line = e.cursor.line.saturating_sub(1)),
    ("w", |e| e.move_to(e.next_word())),
    ("b", |e| e.move_to(e.previous_word())),
    ("e", |e| e.move_to(e.word_end())),
    ("0", |e| e.cursor.col = 0),
    ("^", |e| e.cursor.col = e.first_non_blank()),
    ("$", |e| e.cursor.col = LAST_COL),
    ("gg", |e| e.cursor.line = 0),
    ("G", |e| e.cursor.line = LAST_LINE),
    ("i", |e| e.insert_at(e.cursor.col)),
    ("a", |e| e.insert_at(e.cursor.col + 1)),
    ("I", |e| e.insert_at(e.first_non_blank())),
    ("A", |e| e.insert_at(e.after_last_non_blank())),
    ("o", |e| e.open_line(e.cursor.line + 1)),
    ("O", |e| e.open_line(e.cursor.line)),
    ("x", |e| e.delete(e.cursor.col + 1)),
    ("dw", |e| e.delete(e.word_span_end())),
    ("dd", Editor::delete_line),
    ("yw", |e| e.yank(e.word_span_end())),
    ("yy", Editor::yank_line),
    ("p", Editor::put),
    ("r", |e| e.replacing = true),
    ("u", Editor::undo),
    ("U", Editor::redo),
    (":", |e| e.mode = Mode::Command),
];

/// Commands entered after `:`.
const COMMANDS: &[(&str, Action)] = &[
    ("w", |e| _ = e.write()),
    ("q", Editor::quit),
    ("q!", |e| e.quit = true),
    ("wq", |e| e.quit = e.write()),
    ("x", |e| e.quit = e.write()),
];

/// A key press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Esc,
    Enter,
    Backspace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    /// Entering a `:` command.
    Command,
}

/// A zero-based position. `col` may equal 80 in insert mode only, denoting
/// the position after the last cell.
#[derive(Clone, Copy, Default)]
pub struct Cursor {
    pub line: usize,
    pub col: usize,
}

pub struct Message {
    pub text: String,
    pub is_error: bool,
}

#[derive(Clone)]
enum Register {
    Line(String),
    Symbols(String),
}

pub struct Editor {
    page: Page,
    /// The page as last read or written.
    saved: Page,
    cursor: Cursor,
    mode: Mode,
    command: String,
    message: Option<Message>,
    /// Keys typed so far of an incomplete normal-mode binding.
    keys: String,
    /// Whether `r` awaits its symbol.
    replacing: bool,
    register: Option<Register>,
    undo: Vec<(Page, Cursor)>,
    redo: Vec<(Page, Cursor)>,
    /// Whether the current insert session has recorded its undo state; an
    /// insert session is a single undo step.
    insert_recorded: bool,
    address: Address,
    path: PathBuf,
    quit: bool,
}

impl Editor {
    /// Edits `page`, located at `address` and stored at `path`.
    pub fn new(page: Page, address: Address, path: PathBuf) -> Self {
        Self {
            saved: page.clone(),
            page,
            cursor: Cursor::default(),
            mode: Mode::Normal,
            command: String::new(),
            message: None,
            keys: String::new(),
            replacing: false,
            register: None,
            undo: Vec::new(),
            redo: Vec::new(),
            insert_recorded: false,
            address,
            path,
            quit: false,
        }
    }

    pub fn page(&self) -> &Page {
        &self.page
    }

    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    pub fn address(&self) -> &Address {
        &self.address
    }

    pub fn is_modified(&self) -> bool {
        self.page != self.saved
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn handle_key(&mut self, key: Key) {
        match self.mode {
            Mode::Normal => self.normal_key(key),
            Mode::Insert => self.insert_key(key),
            Mode::Command => self.command_key(key),
        }
    }

    fn normal_key(&mut self, key: Key) {
        self.message = None;
        let Key::Char(c) = key else {
            self.keys.clear();
            self.replacing = false;
            return;
        };
        if std::mem::take(&mut self.replacing) {
            return self.replace(c);
        }
        self.keys.push(c);
        match NORMAL.iter().find(|(keys, _)| keys.starts_with(&self.keys)) {
            Some(&(keys, action)) if keys == self.keys => {
                self.keys.clear();
                action(self);
            }
            Some(_) => {}
            None => self.keys.clear(),
        }
    }

    fn move_to(&mut self, slot: usize) {
        let Cursor { line, col } = from_slot(slot);
        self.cursor = Cursor {
            line,
            col: col.min(LAST_COL),
        };
    }

    fn first_non_blank(&self) -> usize {
        self.page.first_non_blank(self.cursor.line).unwrap_or(0)
    }

    fn after_last_non_blank(&self) -> usize {
        self.page
            .last_non_blank(self.cursor.line)
            .map_or(0, |c| c + 1)
    }

    fn insert_at(&mut self, col: usize) {
        self.cursor.col = col;
        self.mode = Mode::Insert;
        self.insert_recorded = false;
    }

    fn open_line(&mut self, line: usize) {
        if line > LAST_LINE || !self.page.is_line_blank(LAST_LINE) {
            return self.show("no room for a new line", true);
        }
        self.begin_change();
        self.page.open_line(line);
        self.cursor = Cursor { line, col: 0 };
        self.mode = Mode::Insert;
        self.insert_recorded = true;
    }

    /// The column `dw` and `yw` stop at: the next word on the line, or else
    /// just past the last non-blank, so that trailing blanks are excluded.
    fn word_span_end(&self) -> usize {
        let Cursor { line, col } = self.cursor;
        let next = from_slot(self.next_word());
        if next.line == line && next.col < CHARS_PER_LINE {
            return next.col;
        }
        self.page
            .last_non_blank(line)
            .map_or(0, |c| c + 1)
            .max(col + 1)
    }

    /// Copies columns from the cursor to `end` into the register.
    fn yank(&mut self, end: usize) {
        let Cursor { line, col } = self.cursor;
        self.register = Some(Register::Symbols(self.page.line(line)[col..end].to_owned()));
    }

    /// Deletes columns from the cursor to `end` into the register.
    fn delete(&mut self, end: usize) {
        self.yank(end);
        self.begin_change();
        self.page
            .delete_chars(self.cursor.line, self.cursor.col, end);
    }

    fn yank_line(&mut self) {
        self.register = Some(Register::Line(self.page.line(self.cursor.line).to_owned()));
    }

    fn delete_line(&mut self) {
        self.yank_line();
        self.begin_change();
        self.page.delete_line(self.cursor.line);
    }

    fn replace(&mut self, c: char) {
        if !is_allowed(c) {
            return self.reject(c);
        }
        self.begin_change();
        self.page.set(self.cursor.line, self.cursor.col, c);
    }

    /// Overwrites the current line with a stored line, or the cells from the
    /// cursor with stored symbols, truncated at the end of the line.
    fn put(&mut self) {
        let Some(register) = self.register.clone() else {
            return self.show("the register is empty", true);
        };
        self.begin_change();
        let Cursor { line, col } = self.cursor;
        match register {
            Register::Line(text) => {
                for (i, c) in text.chars().enumerate() {
                    self.page.set(line, i, c);
                }
                self.cursor.col = self.page.first_non_blank(line).unwrap_or(0);
            }
            Register::Symbols(text) => {
                let n = text.len().min(CHARS_PER_LINE - col);
                for (i, c) in text.chars().take(n).enumerate() {
                    self.page.set(line, col + i, c);
                }
                self.cursor.col = col + n - 1;
            }
        }
    }

    fn insert_key(&mut self, key: Key) {
        self.message = None;
        let Cursor { line, col } = self.cursor;
        match key {
            Key::Esc => {
                self.mode = Mode::Normal;
                self.cursor.col = col.saturating_sub(1);
            }
            Key::Char(c) if is_allowed(c) => self.type_symbol(c),
            Key::Char(c) => self.reject(c),
            Key::Enter if line < LAST_LINE => {
                self.cursor = Cursor {
                    line: line + 1,
                    col: 0,
                }
            }
            Key::Backspace if (line, col) != (0, 0) => {
                self.cursor = match col {
                    0 => Cursor {
                        line: line - 1,
                        col: LAST_COL,
                    },
                    _ => Cursor { line, col: col - 1 },
                };
                self.record_insert();
                self.page.set(self.cursor.line, self.cursor.col, ' ');
            }
            _ => {}
        }
    }

    /// Writes `c` at the cursor and advances, wrapping to the next line.
    fn type_symbol(&mut self, c: char) {
        let Cursor { mut line, mut col } = self.cursor;
        if col == CHARS_PER_LINE {
            if line == LAST_LINE {
                return self.show("end of the page", true);
            }
            (line, col) = (line + 1, 0);
        }
        self.record_insert();
        self.page.set(line, col, c);
        self.cursor = Cursor { line, col: col + 1 };
    }

    fn record_insert(&mut self) {
        if !self.insert_recorded {
            self.begin_change();
            self.insert_recorded = true;
        }
    }

    fn command_key(&mut self, key: Key) {
        match key {
            Key::Esc => {
                self.mode = Mode::Normal;
                self.command.clear();
            }
            Key::Enter => {
                self.mode = Mode::Normal;
                let command = std::mem::take(&mut self.command);
                self.run_command(&command);
            }
            Key::Backspace => {
                if self.command.pop().is_none() {
                    self.mode = Mode::Normal;
                }
            }
            Key::Char(c) => self.command.push(c),
        }
    }

    fn run_command(&mut self, command: &str) {
        match COMMANDS.iter().find(|(name, _)| *name == command) {
            Some(&(_, action)) => action(self),
            None if command.is_empty() => {}
            None => self.show(format!("unknown command :{command}"), true),
        }
    }

    fn quit(&mut self) {
        if self.is_modified() {
            return self.show("unsaved changes (:q! discards them)", true);
        }
        self.quit = true;
    }

    /// Saves the page; returns whether it succeeded.
    fn write(&mut self) -> bool {
        match self.page.save(&self.path) {
            Ok(()) => {
                self.saved = self.page.clone();
                self.show("written", false);
                true
            }
            Err(err) => {
                self.show(format!("{err:#}"), true);
                false
            }
        }
    }

    /// Records the state before a change, for undo.
    fn begin_change(&mut self) {
        if self.undo.len() == UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.undo.push((self.page.clone(), self.cursor));
        self.redo.clear();
    }

    fn undo(&mut self) {
        if let Some(state) = self.undo.pop() {
            let current = self.restore(state);
            self.redo.push(current);
        }
    }

    fn redo(&mut self) {
        if let Some(state) = self.redo.pop() {
            let current = self.restore(state);
            self.undo.push(current);
        }
    }

    /// Replaces the page and cursor with `state`, returning the current ones.
    fn restore(&mut self, (page, cursor): (Page, Cursor)) -> (Page, Cursor) {
        let current = (std::mem::replace(&mut self.page, page), self.cursor);
        self.cursor = Cursor {
            col: cursor.col.min(LAST_COL),
            ..cursor
        };
        current
    }

    fn reject(&mut self, c: char) {
        self.show(
            format!("{c:?} is not in the alphabet ({ALPHABET_DESCRIPTION})"),
            true,
        );
    }

    fn show(&mut self, text: impl Into<String>, is_error: bool) {
        self.message = Some(Message {
            text: text.into(),
            is_error,
        });
    }

    // Word motions treat the page as one sequence of *slots*, each line
    // followed by a virtual blank slot so that no word spans two lines:
    // slot = line × 81 + col.

    fn next_word(&self) -> usize {
        let mut slot = to_slot(self.cursor);
        let start = self.class(slot);
        while slot < LAST_SLOT && start != Class::Blank && self.class(slot) == start {
            slot += 1;
        }
        while slot < LAST_SLOT && self.class(slot) == Class::Blank {
            slot += 1;
        }
        slot
    }

    fn word_end(&self) -> usize {
        let mut slot = (to_slot(self.cursor) + 1).min(LAST_SLOT);
        while slot < LAST_SLOT && self.class(slot) == Class::Blank {
            slot += 1;
        }
        let class = self.class(slot);
        while class != Class::Blank && slot < LAST_SLOT && self.class(slot + 1) == class {
            slot += 1;
        }
        slot
    }

    fn previous_word(&self) -> usize {
        let mut slot = to_slot(self.cursor).saturating_sub(1);
        while slot > 0 && self.class(slot) == Class::Blank {
            slot -= 1;
        }
        let class = self.class(slot);
        while class != Class::Blank && slot > 0 && self.class(slot - 1) == class {
            slot -= 1;
        }
        slot
    }

    fn class(&self, slot: usize) -> Class {
        let Cursor { line, col } = from_slot(slot);
        match col {
            CHARS_PER_LINE => Class::Blank,
            _ => match self.page.get(line, col) {
                ' ' => Class::Blank,
                ',' | '.' => Class::Punctuation,
                _ => Class::Letter,
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Blank,
    Letter,
    Punctuation,
}

const SLOTS_PER_LINE: usize = CHARS_PER_LINE + 1;
const LAST_SLOT: usize = LINES_PER_PAGE * SLOTS_PER_LINE - 1;

fn to_slot(cursor: Cursor) -> usize {
    cursor.line * SLOTS_PER_LINE + cursor.col
}

fn from_slot(slot: usize) -> Cursor {
    Cursor {
        line: slot / SLOTS_PER_LINE,
        col: slot % SLOTS_PER_LINE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(lines: &[&str]) -> Editor {
        let page = Page::from_text(&lines.join("\n")).unwrap();
        let address = Address::new([0; 64], 1, 1, 1, 1).unwrap();
        Editor::new(page, address, PathBuf::from("/nonexistent/page"))
    }

    /// Sends `text` as keys; ESC, LF and BS stand for Esc, Enter and
    /// Backspace.
    fn keys(ed: &mut Editor, text: &str) {
        for c in text.chars() {
            ed.handle_key(match c {
                '\x1b' => Key::Esc,
                '\n' => Key::Enter,
                '\x08' => Key::Backspace,
                c => Key::Char(c),
            });
        }
    }

    fn at(ed: &Editor) -> (usize, usize) {
        (ed.cursor.line, ed.cursor.col)
    }

    fn line(ed: &Editor, n: usize) -> &str {
        ed.page().line(n).trim_end()
    }

    #[test]
    fn motions_stay_on_the_page() {
        let mut ed = editor(&[]);
        keys(&mut ed, "hk");
        assert_eq!(at(&ed), (0, 0));
        keys(&mut ed, "jjjl");
        assert_eq!(at(&ed), (3, 1));
        keys(&mut ed, "G$jl");
        assert_eq!(at(&ed), (39, 79));
        keys(&mut ed, "gg0");
        assert_eq!(at(&ed), (0, 0));
    }

    #[test]
    fn counts_are_not_interpreted() {
        let mut ed = editor(&[]);
        keys(&mut ed, "3j");
        assert_eq!(at(&ed), (1, 0));
    }

    #[test]
    fn insert_overwrites_anywhere() {
        let mut ed = editor(&["xxxxxx"]);
        keys(&mut ed, "lliab\x1b");
        assert_eq!(line(&ed, 0), "xxabxx");
        assert_eq!(at(&ed), (0, 3));
        keys(&mut ed, "jjjjjlllllihi\x1b");
        assert_eq!(&ed.page().line(5)[..11], "        hi ");
        assert!(ed.is_modified());
    }

    #[test]
    fn foreign_symbols_are_refused() {
        let mut ed = editor(&[]);
        keys(&mut ed, "iHello! 123 world?\x1b");
        assert_eq!(line(&ed, 0), "ello  world");
        keys(&mut ed, "0rX");
        assert_eq!(ed.page().get(0, 0), 'e');
        assert!(ed.message().unwrap().is_error);
        keys(&mut ed, "rj");
        assert_eq!(ed.page().get(0, 0), 'j');
    }

    #[test]
    fn typing_wraps_and_stops_at_end_of_page() {
        let mut ed = editor(&[]);
        keys(&mut ed, "$ib");
        assert_eq!(at(&ed), (0, 80));
        keys(&mut ed, "c\x1b");
        assert_eq!((ed.page().get(0, 79), ed.page().get(1, 0)), ('b', 'c'));
        keys(&mut ed, "G$az");
        assert_eq!(ed.page().get(39, 79), ' ');
        assert!(ed.message().unwrap().is_error);
    }

    #[test]
    fn enter_and_backspace() {
        let mut ed = editor(&[]);
        keys(&mut ed, "iab\ncd\x08\x08\x08z\x1b");
        assert_eq!(line(&ed, 0), format!("ab{}z", " ".repeat(77)));
        assert!(ed.page().is_line_blank(1));
    }

    #[test]
    fn append_and_insert_at_ends() {
        let mut ed = editor(&["  hello"]);
        keys(&mut ed, "A, world\x1b");
        assert_eq!(line(&ed, 0), "  hello, world");
        keys(&mut ed, "Ioh \x1b");
        assert_eq!(line(&ed, 0), "  oh lo, world");
        keys(&mut ed, "0ax\x1b");
        assert_eq!(line(&ed, 0), " xoh lo, world");
    }

    #[test]
    fn x_r_dd() {
        let mut ed = editor(&["abc", "def", "ghi"]);
        keys(&mut ed, "x");
        assert_eq!(line(&ed, 0), "bc");
        keys(&mut ed, "jrz");
        assert_eq!(line(&ed, 1), "zef");
        keys(&mut ed, "dd");
        assert_eq!(line(&ed, 1), "ghi");
        keys(&mut ed, "dx");
        assert_eq!(line(&ed, 1), "ghi");
    }

    #[test]
    fn dw() {
        let mut ed = editor(&["the library, is total.", "next"]);
        keys(&mut ed, "dw");
        assert_eq!(line(&ed, 0), "library, is total.");
        keys(&mut ed, "edw");
        assert_eq!(line(&ed, 0), "librar, is total.");
        keys(&mut ed, "wdw");
        assert_eq!(line(&ed, 0), "librar, total.");
        // The last word: delete to the end of the line only.
        keys(&mut ed, "$bdw");
        assert_eq!(line(&ed, 0), "librar, total");
        keys(&mut ed, "bdw");
        assert_eq!(line(&ed, 0), "librar,");
        assert_eq!(line(&ed, 1), "next");
        // On the last line too, trailing blanks stay out of the register.
        let mut ed = editor(&[]);
        keys(&mut ed, "Giend\x1b0yw");
        assert!(matches!(&ed.register, Some(Register::Symbols(s)) if s == "end"));
    }

    #[test]
    fn put_line() {
        let mut ed = editor(&["abc", "def"]);
        keys(&mut ed, "p");
        assert!(ed.message().unwrap().is_error);
        keys(&mut ed, "ddjp");
        assert_eq!((line(&ed, 0), line(&ed, 1)), ("def", "abc"));
        keys(&mut ed, "kp");
        assert_eq!(line(&ed, 0), "abc");
        keys(&mut ed, "u");
        assert_eq!(line(&ed, 0), "def");
    }

    #[test]
    fn put_symbols() {
        let mut ed = editor(&["ab cd", "xxxxxxxx"]);
        keys(&mut ed, "xjlp");
        assert_eq!(line(&ed, 1), "xaxxxxxx");
        keys(&mut ed, "k0dwjp");
        assert_eq!(line(&ed, 0), "cd");
        assert_eq!(line(&ed, 1), "b xxxxxx");
        assert_eq!(at(&ed), (1, 1));
        keys(&mut ed, "$p");
        assert_eq!(ed.page().get(1, 79), 'b');
    }

    #[test]
    fn yank() {
        let mut ed = editor(&["the library", "xxxxxxxxxxx"]);
        keys(&mut ed, "ywjp");
        assert_eq!(line(&ed, 1), "the xxxxxxx");
        keys(&mut ed, "kwywj0p");
        assert_eq!(line(&ed, 1), "libraryxxxx");
        keys(&mut ed, "kyyjp");
        assert_eq!(line(&ed, 1), "the library");
        keys(&mut ed, "yd");
        assert_eq!(line(&ed, 0), "the library");
        // Only the three puts are changes.
        assert_eq!(ed.undo.len(), 3);
    }

    #[test]
    fn open_lines() {
        let mut ed = editor(&["one", "three"]);
        keys(&mut ed, "otwo\x1b");
        assert_eq!((line(&ed, 1), line(&ed, 2)), ("two", "three"));
        keys(&mut ed, "ggOzero\x1b");
        assert_eq!((line(&ed, 0), line(&ed, 3)), ("zero", "three"));

        let mut full = editor(&["a"; 40]);
        keys(&mut full, "o");
        assert_eq!(full.mode(), Mode::Normal);
        assert!(full.message().unwrap().is_error);
    }

    #[test]
    fn undo_redo() {
        let mut ed = editor(&["abc"]);
        keys(&mut ed, "ixyz\x1bx");
        assert_eq!(line(&ed, 0), "xy");
        keys(&mut ed, "u");
        assert_eq!(line(&ed, 0), "xyz");
        keys(&mut ed, "uu");
        assert_eq!(line(&ed, 0), "abc");
        assert!(!ed.is_modified());
        keys(&mut ed, "U");
        assert_eq!(line(&ed, 0), "xyz");
    }

    #[test]
    fn word_motions() {
        let mut ed = editor(&["the library, is total.", "", "  its shelves"]);
        for (key, pos) in [
            ('w', (0, 4)),
            ('w', (0, 11)),
            ('w', (0, 13)),
            ('e', (0, 14)),
            ('w', (0, 16)),
            ('w', (0, 21)),
            ('w', (2, 2)),
            ('b', (0, 21)),
            ('b', (0, 16)),
            ('G', (39, 16)),
            ('w', (39, 79)),
            ('e', (39, 79)),
        ] {
            ed.handle_key(Key::Char(key));
            assert_eq!(at(&ed), pos, "after {key}");
        }
        keys(&mut ed, "gg0b");
        assert_eq!(at(&ed), (0, 0));
    }

    #[test]
    fn caret_and_dollar() {
        let mut ed = editor(&["   abc"]);
        keys(&mut ed, "^");
        assert_eq!(at(&ed), (0, 3));
        keys(&mut ed, "$");
        assert_eq!(at(&ed), (0, 79));
    }

    #[test]
    fn commands() {
        let mut ed = editor(&[]);
        keys(&mut ed, ":bogus\n");
        assert!(ed.message().unwrap().is_error);
        keys(&mut ed, ":\x08");
        assert_eq!(ed.mode(), Mode::Normal);
        keys(&mut ed, ":wq\x1b:");
        assert_eq!(ed.command(), "");
        keys(&mut ed, "\x1b");
        keys(&mut ed, "ia\x1b:q\n");
        assert!(!ed.should_quit());
        keys(&mut ed, ":w\n");
        assert!(ed.message().unwrap().is_error);
        keys(&mut ed, ":q!\n");
        assert!(ed.should_quit());
    }

    #[test]
    fn write_and_quit() {
        let dir = std::env::temp_dir().join(format!("vibabel-editor-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("page");
        let address = Address::new([0; 64], 1, 1, 1, 1).unwrap();
        let mut ed = Editor::new(Page::blank(), address, path.clone());
        keys(&mut ed, "ihello\x1b:w\n");
        assert!(!ed.is_modified());
        assert_eq!(Page::load(&path).unwrap().line(0).trim_end(), "hello");
        keys(&mut ed, "x:x\n");
        assert!(ed.should_quit());
        assert_eq!(Page::load(&path).unwrap().line(0).trim_end(), "hell");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Random pages and key sequences. After every key the cursor is on the
    /// page and the pending keys prefix a binding; afterwards, undoing every
    /// change restores the original page and redoing them all restores the
    /// final one. `VIBABEL_FUZZ=N` sets the number of runs.
    #[test]
    fn random_keys() {
        const POOL: &[u8] = b"hjklwbe0^$gGiaIAoOxdyprUu:qwx! ,.zZ\x1b\x1b\n\n\x08\x08";
        let runs = std::env::var("VIBABEL_FUZZ").map_or(300, |n| n.parse().unwrap());
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut random = |n: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as usize % n
        };
        for run in 0..runs {
            let mut page = Page::blank();
            for line in 0..LINES_PER_PAGE {
                let len = [0, random(CHARS_PER_LINE), CHARS_PER_LINE][random(3)];
                for col in 0..len {
                    page.set(line, col, char::from(b"ab ,."[random(5)]));
                }
            }
            let address = Address::new([0; 64], 1, 1, 1, 1).unwrap();
            let mut ed = Editor::new(page.clone(), address, PathBuf::from("/nonexistent/page"));
            let mut sent = String::new();
            for _ in 0..400 {
                let c = char::from(POOL[random(POOL.len())]);
                sent.push(c);
                keys(&mut ed, &c.to_string());
                let Cursor { line, col } = ed.cursor;
                let max_col = if ed.mode == Mode::Insert {
                    CHARS_PER_LINE
                } else {
                    LAST_COL
                };
                assert!(line <= LAST_LINE && col <= max_col, "run {run}: {sent:?}");
                assert!(
                    NORMAL.iter().any(|(k, _)| k.starts_with(&ed.keys)),
                    "run {run}: {sent:?}"
                );
            }
            keys(&mut ed, "\x1b\x1b");
            while !ed.redo.is_empty() {
                keys(&mut ed, "U");
            }
            let last = ed.page.clone();
            while !ed.undo.is_empty() {
                keys(&mut ed, "u");
            }
            assert_eq!(ed.page, page, "run {run}: undo: {sent:?}");
            while !ed.redo.is_empty() {
                keys(&mut ed, "U");
            }
            assert_eq!(ed.page, last, "run {run}: redo: {sent:?}");
        }
    }
}
