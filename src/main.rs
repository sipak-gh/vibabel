//! Command-line interface.
//!
//! ```text
//! vibabel edit PAGE_ID
//! vibabel edit -x HEXAGON -w WALL -s SHELF -b BOOK -p PAGE
//! vibabel pageid -x HEXAGON -w WALL -s SHELF -b BOOK -p PAGE
//! vibabel decode PAGE_ID
//! ```

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Result, ensure};
use clap::{Args, Parser, Subcommand};

use vibabel::address::{Address, parse_hex, parse_hexagon};
use vibabel::editor::Editor;
use vibabel::library::HEXAGON_BYTES;
use vibabel::page::Page;
use vibabel::terminal;

/// A vi-like editor for pages of the Library of Babel.
///
/// Locations are given in hexadecimal. Pages are stored in the current
/// directory under their page ID.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Edit the page given by a page ID or by its location.
    Edit {
        #[arg(required_unless_present = "hexagon", conflicts_with = "Location")]
        page_id: Option<String>,
        #[command(flatten)]
        location: Option<Location>,
    },
    /// Print the page ID of a location.
    Pageid {
        #[command(flatten)]
        location: Location,
    },
    /// Print the location of a page ID.
    Decode { page_id: String },
}

#[derive(Args)]
struct Location {
    /// Hexagon, at most 128 hex digits.
    #[arg(short = 'x', long, value_name = "HEX", value_parser = parse_hexagon)]
    hexagon: [u8; HEXAGON_BYTES],
    /// Wall, 0x1-0x4.
    #[arg(short, long, value_name = "HEX", value_parser = parse_hex)]
    wall: u32,
    /// Shelf, 0x1-0x5.
    #[arg(short, long, value_name = "HEX", value_parser = parse_hex)]
    shelf: u32,
    /// Book, 0x1-0x20.
    #[arg(short, long, value_name = "HEX", value_parser = parse_hex)]
    book: u32,
    /// Page, 0x1-0x19a.
    #[arg(short, long, value_name = "HEX", value_parser = parse_hex)]
    page: u32,
}

impl Location {
    fn address(&self) -> Result<Address> {
        Address::new(self.hexagon, self.wall, self.shelf, self.book, self.page)
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Pageid { location } => println!("{}", location.address()?.to_short_id()),
        Command::Decode { page_id } => println!("{}", Address::from_page_id(&page_id)?),
        Command::Edit { page_id, location } => {
            let address = match page_id {
                Some(id) => Address::from_page_id(&id)?,
                None => location.expect("enforced by clap").address()?,
            };
            edit(address)?;
        }
    }
    Ok(())
}

fn edit(address: Address) -> Result<()> {
    ensure!(
        std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
        "edit requires a terminal"
    );
    let path = PathBuf::from(address.to_short_id());
    // Pages saved under the full ID by earlier versions move to the short name.
    let legacy = PathBuf::from(address.to_page_id());
    if !path.exists() && legacy.exists() {
        std::fs::rename(&legacy, &path)?;
    }
    let mut editor = Editor::new(Page::load(&path)?, address, path);
    terminal::run(&mut editor)
}
