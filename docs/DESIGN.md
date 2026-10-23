# Design

## Scope

vibabel edits one page at a time. It implements a subset of vi sufficient
for composing text in a fixed grid: single-cell and word motions, overwrite
insertion, symbol, word and line deletion and copying, a single register,
undo and redo, and the commands to write and quit. Counts, searching,
visual mode and multiple registers are excluded.

## Page model

A page is a 40 × 80 array of bytes, each holding a symbol of the alphabet,
initially blank. The page has no length and no end of line: every cell is
addressable at all times. `Page::set` is the only operation that writes a
symbol and rejects any symbol outside the alphabet, so no sequence of
operations can introduce one.

Because the page cannot grow or shrink, operations that would change its
size in vi are adapted:

| operation | effect                                                        |
|-----------|---------------------------------------------------------------|
| insertion | overwrites; past column 80, continues on the next line        |
| `x`, `dw` | the remainder of the line shifts left; blanks fill the end    |
| `dd`      | lines below shift up; a blank line fills the bottom           |
| `o`, `O`  | lines below shift down; refused unless the last line is blank |
| `p`       | overwrites (see below)                                        |

`Backspace` in insert mode blanks the preceding cell; at the first column,
the preceding cell is the last cell of the previous line.

## Register

`x`, `dw` and `dd` store the deleted text in a single register, tagged as
either symbols or a line; `yw` and `yy` store the same text as `dw` and
`dd` without changing the page. `p` replaces the current line with a
stored line, and writes stored symbols over the cells starting at the
cursor, truncated at the end of the line. The cursor rests on the first
non-blank of a put line, or on the last symbol written. The register is
internal to the process.

`dw` and `yw` span from the cursor to the start of the next word. If no
word follows on the same line, the span ends at the last non-blank of the
line, so that trailing blanks never enter the register.

## Words

Word motions treat the page as one sequence of *slots*, numbering cell
(line, col) as `line × 81 + col`. Slot 80 of each line is a virtual blank,
so no word spans two lines. Symbols fall into three classes (blank, letter,
punctuation), and a word is a maximal run of one non-blank class, as in vi.

## Undo

Each change records a copy of the page (3,200 bytes) and the cursor before
it is applied. An insert session is a single change. The history is bounded
at 1,000 entries; any new change discards the redo history.

## Modes

The editor is a state machine over abstract key events, in normal, insert
or command mode. Normal-mode bindings and `:` commands are declared in two
tables in `editor.rs`, `NORMAL` and `COMMANDS`, each pairing a key sequence
or command name with a function. Typed keys accumulate while they form a
prefix of some binding, run its function on an exact match, and are
discarded otherwise. The function bound to `r` takes the following key as
the replacement symbol. Redo is bound to `U` rather than vi's `Ctrl-r`, so
no binding uses a modifier key; the terminal discards `Ctrl` combinations.

In insert mode the cursor may occupy column 80, the position after the
last cell; leaving insert mode moves it one cell left.

## Architecture

| module      | responsibility                                        |
|-------------|-------------------------------------------------------|
| `library`   | dimensions and alphabet                               |
| `address`   | addresses, hexadecimal parsing, page-ID codec         |
| `page`      | page model, text format, atomic file writes           |
| `editor`    | modes, motions, edits, register, undo                 |
| `terminal`  | key translation and rendering (crossterm)             |
| `main`      | command-line interface (clap)                         |

`editor` has no terminal dependency; `terminal` holds no editing state.
Rendering redraws the whole screen after each key event inside a
synchronized update. When the terminal is smaller than 85 × 45 cells, the
view scrolls to keep the cursor visible.

```text
 [VIBABEL]  HEX 0xDEADBEEF  WALL 1/4  SHELF 2/5  BOOK 3/32  PAGE 4/410
   ┌────────────────────────────────────────┐
 1 │the library is unlimited and cyclical.  │
   ⋮
40 │                                        │
   └────────────────────────────────────────┘
 NORMAL  [+]                         1:39
G9W33eEQD
```

The header shows the hexagon without leading zeros (long values are
abbreviated to their first and last eight hexadecimal digits) and gives the
other components in decimal. The position is right-aligned to the edge of
the page frame. The bottom line shows the current command, a message, or the
short page ID.

## Files

A page is written to a temporary file and renamed over the target, so the
file is never partially written. Reading accepts short lines and fewer
than 40 lines, padding with blanks, and rejects any symbol outside the
alphabet or text beyond the page.

## Verification

- Unit tests cover the alphabet, the page-ID codec (including a fixed
  reference ID and rejection of malformed IDs), page operations and file
  round trips, and each editor command.
- Randomised tests drive the editor with random key sequences over random
  pages, checking after every key that the cursor lies on the page and the
  pending keys prefix a binding, and finally that undoing every change
  restores the original page and redoing them restores the final one.
  Random page IDs must either fail to decode or re-encode to themselves.
  `VIBABEL_FUZZ=N` sets the number of editor runs (default 300, each of
  400 keys).
- `scripts/tui_smoke.py` runs the binary in a pseudo-terminal, sends key
  sequences, reconstructs the rendered frame and checks it against the
  saved file.
