//! Terminal front end: renders the [`Editor`] and supplies its key events.
//!
//! The terminal is placed in raw mode on the alternate screen. Each key
//! event is followed by a full redraw inside a synchronized update. Layout:
//!
//! ```text
//!  VIBABEL  hexagon 0x00000000…deadbeef  wall 1/4  shelf 2/5  book 3/32  page 4/410
//!    ┌──────────── 80 columns ────────────┐
//!  1 │                                    │
//!    ⋮
//! 40 │                                    │
//!    └────────────────────────────────────┘
//!  NORMAL  [+]                         1:1
//! message, command line or page ID
//! ```
//!
//! The full page requires 85 × 45 cells; on smaller terminals the view
//! scrolls to keep the cursor visible.

use std::io::{self, Stdout, Write};

use anyhow::Result;
use crossterm::cursor::{MoveTo, SetCursorStyle, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{
    self, BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::{execute, queue};

use crate::editor::{Editor, Key, Mode};
use crate::library::{
    BOOKS_PER_SHELF, CHARS_PER_LINE, LINES_PER_PAGE, PAGES_PER_BOOK, SHELVES_PER_WALL,
    WALLS_PER_HEXAGON,
};

const INK: Color = Color::AnsiValue(214);
const FRAME: Color = Color::AnsiValue(240);
const ERROR: Color = Color::AnsiValue(203);

/// Width of the line-number gutter.
const GUTTER: usize = 3;
/// Rows not used by page text: header, two borders, status, message.
const CHROME_ROWS: usize = 5;
/// Columns not used by page text: gutter and two borders.
const CHROME_COLS: usize = GUTTER + 2;

/// Runs the editor until it quits.
pub fn run(editor: &mut Editor) -> Result<()> {
    let _guard = TerminalGuard::enter()?;
    let mut out = io::stdout();
    let mut view = View::default();
    while !editor.should_quit() {
        draw(&mut out, editor, &mut view)?;
        if let Event::Key(event) = event::read()?
            && event.kind != KeyEventKind::Release
            && let Some(key) = translate(event)
        {
            editor.handle_key(key);
        }
    }
    Ok(())
}

fn translate(event: KeyEvent) -> Option<Key> {
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    Some(match event.code {
        KeyCode::Char(c) if !ctrl => Key::Char(c),
        KeyCode::Esc => Key::Esc,
        KeyCode::Enter => Key::Enter,
        KeyCode::Backspace => Key::Backspace,
        _ => return None,
    })
}

/// Holds the terminal in raw mode on the alternate screen; restores it on
/// drop and on panic.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal();
            hook(info);
        }));
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    let _ = execute!(
        io::stdout(),
        SetCursorStyle::DefaultUserShape,
        Show,
        LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
}

/// The first page line and column on screen.
#[derive(Default)]
struct View {
    top: usize,
    left: usize,
}

/// Returns the new start of a `window`-sized range over `0..total` that
/// contains `pos`, moving the previous `start` as little as possible.
fn scroll(start: usize, pos: usize, window: usize, total: usize) -> usize {
    start
        .clamp(pos.saturating_sub(window - 1), pos)
        .min(total - window)
}

fn draw(out: &mut Stdout, editor: &Editor, view: &mut View) -> Result<()> {
    let (width, height) = terminal::size()?;
    let (width, height) = (usize::from(width), usize::from(height));
    queue!(out, BeginSynchronizedUpdate, MoveTo(0, 0))?;
    if width <= CHROME_COLS || height <= CHROME_ROWS {
        queue!(out, Clear(ClearType::All), Print("terminal too small"))?;
        queue!(out, EndSynchronizedUpdate)?;
        return Ok(out.flush()?);
    }

    let rows = (height - CHROME_ROWS).min(LINES_PER_PAGE);
    let cols = (width - CHROME_COLS).min(CHARS_PER_LINE);
    let cursor = editor.cursor();
    view.top = scroll(view.top, cursor.line, rows, LINES_PER_PAGE);
    view.left = scroll(
        view.left,
        cursor.col.min(CHARS_PER_LINE - 1),
        cols,
        CHARS_PER_LINE,
    );

    let a = editor.address();
    let header = format!(
        " [VIBABEL]  HEX 0x{}  WALL {}/{WALLS_PER_HEXAGON}  SHELF {}/{SHELVES_PER_WALL}  BOOK {}/{BOOKS_PER_SHELF}  PAGE {}/{PAGES_PER_BOOK}",
        a.hexagon_short().to_uppercase(),
        a.wall,
        a.shelf,
        a.book,
        a.page,
    );
    queue!(
        out,
        SetForegroundColor(INK),
        SetAttribute(Attribute::Reverse),
        Print(fit(&header, width)),
        SetAttribute(Attribute::NoReverse),
    )?;

    border(out, 1, '┌', '┐', cols)?;
    for row in 0..rows {
        let line = view.top + row;
        let number = if line == cursor.line { INK } else { FRAME };
        queue!(
            out,
            MoveTo(0, (2 + row) as u16),
            SetForegroundColor(number),
            Print(format!("{:>2} ", line + 1)),
            SetForegroundColor(FRAME),
            Print('│'),
            SetForegroundColor(INK),
            Print(&editor.page().line(line)[view.left..view.left + cols]),
            SetForegroundColor(FRAME),
            Print('│'),
            Clear(ClearType::UntilNewLine),
        )?;
    }
    border(out, 2 + rows, '└', '┘', cols)?;

    let (label, shape) = match editor.mode() {
        Mode::Normal => (" NORMAL ", SetCursorStyle::SteadyBlock),
        Mode::Insert => (" INSERT ", SetCursorStyle::SteadyBar),
        Mode::Command => (" COMMAND ", SetCursorStyle::SteadyBar),
    };
    let modified = if editor.is_modified() { " [+]" } else { "" };
    let position = format!(
        "{}:{}",
        cursor.line + 1,
        cursor.col.min(CHARS_PER_LINE - 1) + 1
    );
    let edge = (GUTTER + cols + 2).min(width);
    let rest = edge.saturating_sub(label.len());
    queue!(
        out,
        MoveTo(0, (3 + rows) as u16),
        SetForegroundColor(INK),
        SetAttribute(Attribute::Reverse),
        Print(fit(label, label.len().min(width))),
        SetAttribute(Attribute::NoReverse),
        Print(fit(
            &format!(
                "{modified}{position:>w$}",
                w = rest.saturating_sub(modified.len())
            ),
            rest
        )),
    )?;

    let bottom = (4 + rows) as u16;
    let (color, text) = match (editor.mode(), editor.message()) {
        (Mode::Command, _) => (INK, format!(":{}", editor.command())),
        (_, Some(m)) => (if m.is_error { ERROR } else { INK }, m.text.clone()),
        (_, None) => (FRAME, editor.address().to_short_id()),
    };
    queue!(
        out,
        MoveTo(0, bottom),
        SetForegroundColor(color),
        Print(fit(&text, width)),
        ResetColor,
        Clear(ClearType::FromCursorDown),
    )?;

    let (x, y) = match editor.mode() {
        Mode::Command => (
            (1 + editor.command().chars().count()).min(width - 1),
            bottom,
        ),
        _ => (
            GUTTER + 1 + cursor.col - view.left,
            (2 + cursor.line - view.top) as u16,
        ),
    };
    queue!(out, shape, MoveTo(x as u16, y), Show, EndSynchronizedUpdate)?;
    Ok(out.flush()?)
}

/// Draws the top or bottom border of the page on screen row `y`.
fn border(out: &mut Stdout, y: usize, left: char, right: char, cols: usize) -> io::Result<()> {
    queue!(
        out,
        MoveTo(0, y as u16),
        SetForegroundColor(FRAME),
        Print(format!("{:GUTTER$}{left}{}{right}", "", "─".repeat(cols))),
        Clear(ClearType::UntilNewLine),
    )
}

/// Truncates or pads `text` to `width` characters.
fn fit(text: &str, width: usize) -> String {
    format!("{:width$.width$}", text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_keeps_position_visible() {
        assert_eq!(scroll(0, 5, 10, 40), 0);
        assert_eq!(scroll(0, 15, 10, 40), 6);
        assert_eq!(scroll(6, 3, 10, 40), 3);
        assert_eq!(scroll(0, 39, 40, 40), 0);
        assert_eq!(scroll(35, 39, 10, 40), 30);
    }

    #[test]
    fn fit_pads_and_truncates() {
        assert_eq!(fit("abc", 5), "abc  ");
        assert_eq!(fit("abcdef", 3), "abc");
        assert_eq!(fit("a…b", 2), "a…");
    }

    #[test]
    fn translate_keys() {
        let key = |code, modifiers| translate(KeyEvent::new(code, modifiers));
        assert_eq!(key(KeyCode::Char('r'), KeyModifiers::CONTROL), None);
        assert_eq!(key(KeyCode::Left, KeyModifiers::NONE), None);
        assert_eq!(
            key(KeyCode::Char('Q'), KeyModifiers::SHIFT),
            Some(Key::Char('Q'))
        );
    }
}
