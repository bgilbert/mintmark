# Mintmark

Driver for an Epson TM-U220B receipt printer, taking Markdown as input.

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

- Images (rendered as the alt text)
- Links (rendered as the link text)
- Tables
- Footnotes
- Definition lists
- Task lists
