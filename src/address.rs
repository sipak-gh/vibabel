//! Page addresses and the page-ID encoding.
//!
//! An [`Address`] is the tuple (hexagon, wall, shelf, book, page). The
//! hexagon is an unsigned 512-bit integer; the other components are 1-based.
//!
//! # Page ID
//!
//! The address is serialised, most significant bit first, as
//!
//! ```text
//! 000 | hexagon (512) | wall−1 (2) | shelf−1 (3) | book−1 (5) | page−1 (9)
//! ```
//!
//! and the resulting 534 bits are written as 89 digits of the URL-safe
//! base64 alphabet (RFC 4648 §5) without padding. The ID is a valid file
//! name, and its first digit lies in `A`–`H`, so it is never read as a
//! command-line option. Decoding rejects non-zero padding and out-of-range
//! components; the encoding is therefore a bijection.

use std::fmt;

use anyhow::{Context, Result, ensure};

use crate::library::{
    BOOKS_PER_SHELF, HEXAGON_BYTES, PAGES_PER_BOOK, SHELVES_PER_WALL, WALLS_PER_HEXAGON,
};

const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

const PAD_BITS: u32 = 3;
const WALL_BITS: u32 = 2;
const SHELF_BITS: u32 = 3;
const BOOK_BITS: u32 = 5;
const PAGE_BITS: u32 = 9;
const ID_BITS: usize =
    (PAD_BITS + 8 * HEXAGON_BYTES as u32 + WALL_BITS + SHELF_BITS + BOOK_BITS + PAGE_BITS) as usize;

/// Length of a page ID in characters.
pub const PAGE_ID_LEN: usize = ID_BITS / 6;

const _: () = assert!(ID_BITS.is_multiple_of(6));
const _: () = assert!(
    WALLS_PER_HEXAGON <= 1 << WALL_BITS
        && SHELVES_PER_WALL <= 1 << SHELF_BITS
        && BOOKS_PER_SHELF <= 1 << BOOK_BITS
        && PAGES_PER_BOOK <= 1 << PAGE_BITS
);

/// The location of one page. Construct with [`Address::new`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Address {
    /// Big-endian.
    pub hexagon: [u8; HEXAGON_BYTES],
    pub wall: u32,
    pub shelf: u32,
    pub book: u32,
    pub page: u32,
}

impl Address {
    /// Validates each 1-based component against the Library's dimensions.
    pub fn new(
        hexagon: [u8; HEXAGON_BYTES],
        wall: u32,
        shelf: u32,
        book: u32,
        page: u32,
    ) -> Result<Self> {
        for (name, value, max) in [
            ("wall", wall, WALLS_PER_HEXAGON),
            ("shelf", shelf, SHELVES_PER_WALL),
            ("book", book, BOOKS_PER_SHELF),
            ("page", page, PAGES_PER_BOOK),
        ] {
            ensure!(
                (1..=max).contains(&value),
                "{name} must be in 1..={max:x} (hex), got {value:x}"
            );
        }
        Ok(Self {
            hexagon,
            wall,
            shelf,
            book,
            page,
        })
    }

    /// Encodes the address as a page ID.
    ///
    /// ```
    /// use vibabel::address::Address;
    /// let a = Address::new([0; 64], 1, 1, 1, 1).unwrap();
    /// assert_eq!(a.to_page_id(), "A".repeat(89));
    /// assert_eq!(Address::from_page_id(&a.to_page_id()).unwrap(), a);
    /// ```
    pub fn to_page_id(&self) -> String {
        let mut bits = Vec::with_capacity(ID_BITS);
        push_bits(&mut bits, 0, PAD_BITS);
        for byte in self.hexagon {
            push_bits(&mut bits, byte.into(), 8);
        }
        push_bits(&mut bits, self.wall - 1, WALL_BITS);
        push_bits(&mut bits, self.shelf - 1, SHELF_BITS);
        push_bits(&mut bits, self.book - 1, BOOK_BITS);
        push_bits(&mut bits, self.page - 1, PAGE_BITS);
        bits.chunks(6)
            .map(|digit| char::from(BASE64URL[take_bits(&mut digit.iter().copied(), 6) as usize]))
            .collect()
    }

    /// The page ID without its leading `A` padding (at least one character).
    ///
    /// ```
    /// use vibabel::address::Address;
    /// let a = Address::new([0; 64], 1, 1, 1, 1).unwrap();
    /// assert_eq!(a.to_short_id(), "A");
    /// ```
    pub fn to_short_id(&self) -> String {
        let full = self.to_page_id();
        let short = full.trim_start_matches('A');
        if short.is_empty() { "A" } else { short }.to_string()
    }

    /// Decodes a page ID, full or shortened (missing leading `A`s are implied).
    pub fn from_page_id(id: &str) -> Result<Self> {
        let len = id.chars().count();
        ensure!(
            (1..=PAGE_ID_LEN).contains(&len),
            "a page ID has at most {PAGE_ID_LEN} characters, got {len}"
        );
        let padded = format!("{}{id}", "A".repeat(PAGE_ID_LEN - len));
        let id = padded.as_str();
        let mut bits = Vec::with_capacity(ID_BITS);
        for c in id.chars() {
            let digit = BASE64URL
                .iter()
                .position(|&b| char::from(b) == c)
                .with_context(|| format!("{c:?} is not a page ID character"))?;
            push_bits(&mut bits, digit as u32, 6);
        }
        let mut bits = bits.into_iter();
        ensure!(
            take_bits(&mut bits, PAD_BITS) == 0,
            "a page ID starts with one of A-H"
        );
        let mut hexagon = [0; HEXAGON_BYTES];
        for byte in &mut hexagon {
            *byte = take_bits(&mut bits, 8) as u8;
        }
        let mut field = |width| take_bits(&mut bits, width) + 1;
        Self::new(
            hexagon,
            field(WALL_BITS),
            field(SHELF_BITS),
            field(BOOK_BITS),
            field(PAGE_BITS),
        )
    }

    /// The hexagon as 128 hex digits.
    pub fn hexagon_hex(&self) -> String {
        self.hexagon.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The hexagon without leading zeros; long values keep their first and last eight digits.
    pub fn hexagon_short(&self) -> String {
        let hex = self.hexagon_hex();
        let hex = hex.trim_start_matches('0');
        match hex.len() {
            0 => "0".into(),
            1..=17 => hex.into(),
            n => format!("{}…{}", &hex[..8], &hex[n - 8..]),
        }
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "hexagon 0x{}", self.hexagon_hex())?;
        writeln!(f, "wall    0x{:x}", self.wall)?;
        writeln!(f, "shelf   0x{:x}", self.shelf)?;
        writeln!(f, "book    0x{:x}", self.book)?;
        write!(f, "page    0x{:x}", self.page)
    }
}

/// Parses a hexagon of at most [`HEXAGON_BYTES`] bytes, ignoring leading
/// zeros.
pub fn parse_hexagon(text: &str) -> Result<[u8; HEXAGON_BYTES]> {
    let digits = hex_digits(text)?.trim_start_matches('0');
    ensure!(
        digits.len() <= 2 * HEXAGON_BYTES,
        "a hexagon has at most {HEXAGON_BYTES} bytes"
    );
    let padded = format!("{digits:0>width$}", width = 2 * HEXAGON_BYTES);
    let mut hexagon = [0; HEXAGON_BYTES];
    for (i, byte) in hexagon.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&padded[2 * i..2 * i + 2], 16)?;
    }
    Ok(hexagon)
}

/// Parses a hexadecimal `u32`.
pub fn parse_hex(text: &str) -> Result<u32> {
    u32::from_str_radix(hex_digits(text)?, 16).with_context(|| format!("{text:?} is too large"))
}

/// Strips an optional `0x` and checks that the rest is hexadecimal.
fn hex_digits(text: &str) -> Result<&str> {
    let digits = text.strip_prefix("0x").unwrap_or(text);
    ensure!(
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_hexdigit()),
        "{text:?} is not a hexadecimal number"
    );
    Ok(digits)
}

fn push_bits(bits: &mut Vec<bool>, value: u32, width: u32) {
    bits.extend((0..width).rev().map(|i| value >> i & 1 == 1));
}

fn take_bits(bits: &mut impl Iterator<Item = bool>, width: u32) -> u32 {
    bits.take(width as usize)
        .fold(0, |acc, bit| acc << 1 | u32::from(bit))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Address {
        let hexagon = std::array::from_fn(|i| (i as u8).wrapping_mul(37).wrapping_add(11));
        Address::new(hexagon, 3, 5, 17, 410).unwrap()
    }

    #[test]
    fn known_id() {
        let a = Address::new(parse_hexagon("deadbeef").unwrap(), 1, 2, 3, 4).unwrap();
        assert_eq!(a.to_page_id(), format!("{}G9W33eEQD", "A".repeat(80)));
    }

    #[test]
    fn short_ids() {
        let a = Address::new(parse_hexagon("deadbeef").unwrap(), 1, 2, 3, 4).unwrap();
        assert_eq!(a.to_short_id(), "G9W33eEQD");
        assert_eq!(Address::from_page_id("G9W33eEQD").unwrap(), a);
        let z = Address::new([0; 64], 1, 1, 1, 1).unwrap();
        assert_eq!(Address::from_page_id(&z.to_short_id()).unwrap(), z);
    }

    #[test]
    fn round_trips() {
        for a in [
            sample(),
            Address::new([0xff; 64], 4, 5, 32, 410).unwrap(),
            Address::new([0; 64], 1, 1, 1, 1).unwrap(),
        ] {
            let id = a.to_page_id();
            assert_eq!(id.len(), PAGE_ID_LEN);
            assert!(('A'..='H').contains(&id.chars().next().unwrap()));
            assert_eq!(Address::from_page_id(&id).unwrap(), a);
        }
    }

    #[test]
    fn rejects_bad_ids() {
        let id = sample().to_page_id();
        assert!(Address::from_page_id("").is_err());
        assert!(Address::from_page_id(&format!("A{id}")).is_err());
        assert!(Address::from_page_id(&format!("Z{}", &id[1..])).is_err());
        assert!(Address::from_page_id(&format!("{}!", &id[..88])).is_err());
        assert!(Address::from_page_id(&format!("{}Ł", &id[..88])).is_err());
        // Page field 0b111111111 = 512.
        assert!(Address::from_page_id(&format!("{}H_", "A".repeat(87))).is_err());
    }

    #[test]
    fn rejects_out_of_range_components() {
        assert!(Address::new([0; 64], 0, 1, 1, 1).is_err());
        assert!(Address::new([0; 64], 5, 1, 1, 1).is_err());
        assert!(Address::new([0; 64], 1, 1, 1, 411).is_err());
    }

    #[test]
    fn parses_hexagons() {
        assert_eq!(parse_hexagon("0").unwrap(), [0; 64]);
        assert_eq!(parse_hexagon("0xabc").unwrap()[62..], [0x0a, 0xbc]);
        assert_eq!(parse_hexagon(&"f".repeat(128)).unwrap(), [0xff; 64]);
        assert!(parse_hexagon(&format!("00{}", "f".repeat(128))).is_ok());
        assert!(parse_hexagon(&"f".repeat(129)).is_err());
        for bad in ["", "0x", "12g", "+1"] {
            assert!(parse_hexagon(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn parses_hex() {
        assert_eq!(parse_hex("19a").unwrap(), 410);
        assert_eq!(parse_hex("0x1").unwrap(), 1);
        for bad in ["", "-1", "+1", "1g", "ffffffffff"] {
            assert!(parse_hex(bad).is_err(), "{bad:?}");
        }
    }

    /// Random IDs either fail to decode or re-encode to themselves.
    #[test]
    fn random_ids() {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut random = |n: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as usize % n
        };
        let mut decoded = 0;
        for _ in 0..100_000 {
            let mut id: String = (0..PAGE_ID_LEN)
                .map(|_| char::from(BASE64URL[random(64)]))
                .collect();
            id.replace_range(..1, &char::from(b'A' + random(9) as u8).to_string());
            if let Ok(a) = Address::from_page_id(&id) {
                assert_eq!(a.to_page_id(), id);
                decoded += 1;
            }
        }
        assert!(decoded > 0);
    }
}
