# Mintmark

Driver for an Epson TM-U220B receipt printer, taking Markdown as input.

[![License](https://img.shields.io/crates/l/mintmark)](https://github.com/bgilbert/mintmark/blob/master/LICENSE)
[![Crate](https://img.shields.io/crates/v/mintmark)](https://crates.io/crates/mintmark)

## Usage

```sh
target/debug/mintmark /dev/usb/lp0 < input.md
```

## Features

- 6 distinct heading types, all centered
- Bold, rendered as double-strike
- Italic, rendered as underline
- Ordered and unordered lists
- Inline code and code blocks, rendered as red
- Strikethrough
- Blockquotes, rendered as indent
- Horizontal rules, rendered by cutting the paper
- Arbitrary 1-bit images, specified as ASCII art in code blocks with the
  `image` language identifier
- QR codes, specified as code blocks with the `qrcode` language identifier
- Code128 code set B barcodes, specified as code blocks with the `code128`
  language identifier

## Missing and non-features

- Paper widths other than 3" ([#6](https://github.com/bgilbert/mintmark/issues/6))
- Images (rendered as the alt text)
- Links (rendered as the link text)
- Tables
- Footnotes
- Definition lists
- Task lists ([#8](https://github.com/bgilbert/mintmark/issues/8))
