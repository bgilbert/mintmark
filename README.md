# Mintmark

Driver for an Epson TM-U220B receipt printer, taking Markdown as input.

## Usage

```sh
target/debug/mintmark < input.md > /dev/usb/lp0
```

## Features

- 6 distinct heading types, all centered
- Bold, rendered as double-strike
- Italic, rendered as underline
- Unordered lists
- Inline code and code blocks, rendered as red
- Strikethrough
- Horizontal rules, rendered by cutting the paper
- QR codes, specified as code blocks with the `qrcode` language identifier

## Missing and non-features

- Ordered lists (rendered as unordered)
- Images (rendered as the alt text)
- Links (rendered as the link text)
- Blockquotes (rendered as plain text)
- Tables
- Footnotes
- Definition lists
- Task lists