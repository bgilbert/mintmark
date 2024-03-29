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
- Inline code and code blocks, rendered as red by default.  Style modifiers
  are specified with the `text` language identifier and one or more
  space-separated keywords: `black`, `bold`, `doubleheight`, `doublewidth`,
  `strikethrough`, `underline`, `wide`
- Strikethrough
- Blockquotes, rendered as indent
- Horizontal rules, rendered by cutting the paper
- Arbitrary 1-bit images, specified as ASCII art in code blocks with the
  `bitmap` language identifier.  Supported keywords: `bold`
- Images in plain PNM format, specified as code blocks with the `image`
  language identifier
- Images in JPEG, PNG, WebP, or raw PNM format, specified as base64-encoded
  images in code blocks with the `image base64` language identifier.  JPEG
  and PNG require the `jpeg` and `png` features, respectively, which are
  enabled by default.
- QR codes, specified as code blocks with the `qrcode` language identifier.
  Supported keywords: `base64`, `bold`
- Code128 code set B barcodes, specified as code blocks with the `code128`
  language identifier.  Supported keywords: `bold`

### Image features

- Red/black can be used with the `bicolor` keyword, e.g.
  `image base64 bicolor`

## Missing and non-features

- Paper widths other than 3" ([#6](https://github.com/bgilbert/mintmark/issues/6))
- Images (rendered as the alt text)
- Links (rendered as the link text)
- Tables
- Footnotes
- Definition lists
- Task lists ([#8](https://github.com/bgilbert/mintmark/issues/8))
