# vibabel

A modal text editor, after vi, for composing pages of the Library of Babel
(Borges, 1941). A page is located by its address in the Library and stored
as a plain-text file named by a page ID derived from that address.

## Constraints

- A page has 40 lines of 80 symbols. Every cell exists from the outset,
  blank; the cursor may occupy any cell, and typing overwrites.
- The alphabet is `a`–`z`, space, comma and period. Other symbols are
  refused.
- Commands take no counts.

## Installation

Requires Linux, a Rust toolchain and a C linker.

```sh
cargo install --path .
```

## Usage

```text
vibabel pageid -x HEXAGON -w WALL -s SHELF -b BOOK -p PAGE
vibabel edit PAGE_ID
vibabel edit -x HEXAGON -w WALL -s SHELF -b BOOK -p PAGE
vibabel decode PAGE_ID
```

| flag             | component | range (hexadecimal)     |
|------------------|-----------|-------------------------|
| `-x` `--hexagon` | hexagon   | at most 128 digits      |
| `-w` `--wall`    | wall      | `1`–`4`                 |
| `-s` `--shelf`   | shelf     | `1`–`5`                 |
| `-b` `--book`    | book      | `1`–`20` (32)           |
| `-p` `--page`    | page      | `1`–`19a` (410)         |

A `0x` prefix is optional. `edit` reads and writes the file named by the short
page ID in the current directory; a missing file is a blank page.

```text
$ vibabel pageid -x deadbeef -w 1 -s 2 -b 3 -p 4
G9W33eEQD
```

Leading `A`s in a page ID are padding: `pageid` prints the shortened ID, and
`decode`/`edit` accept it (or the full 89-character form).

## Keys

| normal mode        | action                                                 |
|--------------------|--------------------------------------------------------|
| `h j k l`          | move one cell                                          |
| `w b e`            | next word, previous word, end of word                  |
| `0 ^ $`            | first column, first non-blank, last column             |
| `gg G`             | first line, last line                                  |
| `i a`              | insert at, after the cursor                            |
| `I A`              | insert at first non-blank, after last non-blank        |
| `o O`              | open a line below, above (the last line must be blank) |
| `x`                | delete the symbol                                      |
| `dw`               | delete to the next word, within the line               |
| `dd`               | delete the line                                        |
| `yw` `yy`          | copy what `dw`, `dd` would delete                      |
| `p`                | put the register                                       |
| `r`*c*             | replace the symbol with *c*                            |
| `u` `U`            | undo, redo                                             |
| `:w :q :q! :wq :x` | write, quit, quit discarding changes, write and quit   |

In insert mode, typing past the last column continues on the next line,
`Enter` moves to the start of the next line, `Backspace` blanks the
preceding cell and `Esc` returns to normal mode.

`x`, `dw` and `dd` store the deleted text, and `yw` and `yy` the copied
text, in a single register. `p` after `dd` or `yy` replaces the current
line; `p` after `x`, `dw` or `yw` overwrites from the cursor, up to the
end of the line.

## File format

40 lines of 80 symbols, each terminated by a newline (3,240 bytes). Short
or missing lines are read as blank.

## Documentation

- [docs/LIBRARY.md](docs/LIBRARY.md): dimensions, alphabet, page-ID encoding.
- [docs/DESIGN.md](docs/DESIGN.md): data model, editing semantics,
  architecture, verification.
- `cargo doc --open`: source documentation.

## Development

```sh
cargo test
VIBABEL_FUZZ=100000 cargo test --release random_keys
cargo clippy --all-targets
cargo build && python3 scripts/tui_smoke.py
```

## License

[0BSD](LICENSE)
