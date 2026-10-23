# The Library

## Dimensions

Borges describes a Library of hexagonal galleries. Four walls of each
hexagon bear five shelves each; each shelf holds thirty-two books of 410
pages; each page has 40 lines of some 80 letters. vibabel adopts these
figures, fixes the line length at 80, and extends the address to the page.

| level   | per parent |
|---------|-----------:|
| hexagon |          — |
| wall    |          4 |
| shelf   |          5 |
| book    |         32 |
| page    |        410 |
| line    |         40 |
| symbol  |         80 |

A page holds 3,200 symbols; there are 29^3200 ≈ 10^4680 distinct pages.
Addressing every page would require identifiers as long as the pages
themselves. vibabel therefore bounds the hexagon number at 512 bits (64
bytes), admitting 2^512 hexagons.

## Alphabet

Borges specifies 25 symbols: 22 letters, space, comma and period. vibabel
uses the 26 letters of the English alphabet in lower case with the same
three marks, 29 symbols in total:

```text
abcdefghijklmnopqrstuvwxyz ,.
```

## Addresses

An address is the tuple (hexagon, wall, shelf, book, page), written in
hexadecimal on the command line. The hexagon is an unsigned integer,
left-padded with zeros; the remaining components count from 1.

| component | range (decimal) | stored (0-based) |
|-----------|-----------------|------------------|
| hexagon   | 0 – 2^512−1     | 512 bits         |
| wall      | 1 – 4           | 2 bits           |
| shelf     | 1 – 5           | 3 bits           |
| book      | 1 – 32          | 5 bits           |
| page      | 1 – 410         | 9 bits           |

## Page IDs

```text
 3     512       2      3      5      9     bits
┌───┬─────────┬──────┬───────┬──────┬──────┐
│000│ hexagon │ wall │ shelf │ book │ page │  534 bits = 89 × 6
└───┴─────────┴──────┴───────┴──────┴──────┘
```

The fields are concatenated most significant bit first, behind three zero
bits, and the 534-bit string is written six bits per digit in the URL-safe
base64 alphabet (`A–Z a–z 0–9 - _`, RFC 4648 §5) without padding, giving
89 characters.

Properties:

- The alphabet excludes `/`, so an ID is a valid file name.
- The leading zero bits restrict the first digit to `A`–`H`; an ID never
  begins with `-` and is not mistaken for a command-line option.
- Decoding rejects non-zero padding and out-of-range components. Every
  address has exactly one ID and every valid ID one address.
- Hexadecimal would require 133 digits. Packing the four small fields
  as a single mixed-radix number gains nothing: 4 × 5 × 32 × 410 = 262,400
  still requires 19 bits.

Hexagon 0, wall 1, shelf 1, book 1, page 1 encodes as 89 `A`s.

Leading `A`s are padding in effect, so user-facing output (`pageid`, the
editor, saved file names) uses the short ID with them removed (at least one
character kept): `G9W33eEQD` stands for 80 `A`s followed by `G9W33eEQD`.
Decoding left-pads a short ID with `A`s to 89 characters. A short ID may
begin with `-`, so pass it after `--` on the command line.
