/*
 * Copyright 2020 Benjamin Gilbert
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
*/

mod render;

use anyhow::{bail, Context, Result};
use barcoders::sym::code128::Code128;
use clap::Parser as ClapParser;
use fs2::FileExt;
use image::imageops::colorops::{dither, BiLevel};
use image::GrayImage;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};
use qrcode::{EcLevel, QrCode};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::rc::Rc;

use render::{Format, FormatFlags, Justification, Renderer};

/// Print Markdown to an Epson TM-U220B receipt printer
#[derive(Debug, ClapParser)]
#[command(version)]
struct Args {
    /// Input file (default: stdin)
    #[arg(long, value_name = "PATH")]
    file: Option<PathBuf>,
    /// Lock file for coordinating exclusive access
    #[arg(long, value_name = "PATH")]
    lock_file: Option<PathBuf>,
    /// Path to the character device node
    #[arg(value_name = "DEVICE-PATH")]
    device: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mut input_bytes: Vec<u8> = Vec::new();
    match args.file {
        Some(path) => OpenOptions::new()
            .read(true)
            .open(path)
            .context("opening input file")?
            .read_to_end(&mut input_bytes)
            .context("reading input file")?,
        None => io::stdin()
            .lock()
            .read_to_end(&mut input_bytes)
            .context("reading stdin")?,
    };
    let input = std::str::from_utf8(&input_bytes).context("couldn't decode input")?;

    let _lockfile = args
        .lock_file
        .map(|path| -> Result<File> {
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .open(path)
                .context("opening lockfile")?;
            file.lock_exclusive().context("locking lockfile")?;
            Ok(file)
        })
        .transpose()?;
    let mut output = OpenOptions::new()
        .read(true)
        .write(true)
        .open(args.device)
        .context("opening output")?;

    render(input, &mut output)
}

fn render(input: &str, output: &mut (impl Read + Write)) -> Result<()> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(input, options);

    let mut renderer = Renderer::new(output);
    let mut code_formats: Vec<FormatInfo> = Vec::new();
    let mut lists: Vec<Option<u64>> = Vec::new();
    for (event, _) in parser.into_offset_iter() {
        match event {
            Event::Start(tag) => {
                match tag {
                    Tag::Paragraph => {}
                    Tag::Heading(level, _, _) => {
                        // Center first.  This only takes effect at the
                        // start of the line, so end tag handling needs to
                        // specially account for it.
                        renderer.set_format(
                            renderer.format().with_justification(Justification::Center),
                        );
                        match level {
                            HeadingLevel::H1 => {
                                renderer.set_format(
                                    renderer.format().with_unidirectional(true).with_flags(
                                        FormatFlags::DOUBLE_HEIGHT
                                            | FormatFlags::DOUBLE_WIDTH
                                            | FormatFlags::EMPHASIZED
                                            | FormatFlags::UNDERLINE,
                                    ),
                                );
                            }
                            HeadingLevel::H2 => {
                                renderer.set_format(
                                    renderer.format().with_unidirectional(true).with_flags(
                                        FormatFlags::DOUBLE_HEIGHT
                                            | FormatFlags::DOUBLE_WIDTH
                                            | FormatFlags::EMPHASIZED,
                                    ),
                                );
                            }
                            HeadingLevel::H3 => {
                                renderer.set_format(
                                    renderer
                                        .format()
                                        .with_flags(
                                            FormatFlags::EMPHASIZED | FormatFlags::UNDERLINE,
                                        )
                                        .without_flags(FormatFlags::NARROW),
                                );
                            }
                            HeadingLevel::H4 => {
                                renderer.set_format(
                                    renderer
                                        .format()
                                        .with_flags(FormatFlags::EMPHASIZED)
                                        .without_flags(FormatFlags::NARROW),
                                );
                            }
                            HeadingLevel::H5 => {
                                renderer.set_format(
                                    renderer.format().with_flags(
                                        FormatFlags::EMPHASIZED | FormatFlags::UNDERLINE,
                                    ),
                                );
                            }
                            _ => {
                                renderer.set_format(
                                    renderer.format().with_flags(FormatFlags::EMPHASIZED),
                                );
                            }
                        }
                    }
                    Tag::BlockQuote => {
                        renderer.set_format(renderer.format().with_added_indent(4));
                    }
                    Tag::CodeBlock(kind) => {
                        let info = match kind {
                            CodeBlockKind::Indented => "".into(),
                            CodeBlockKind::Fenced(s) => s,
                        };
                        let info = FormatInfo::parse(&info);
                        match info.language.as_ref() {
                            "bitmap" => {}
                            "image" => {}
                            "qrcode" => {}
                            "code128" => {}
                            _ => {
                                let mut format = renderer.format().with_red(true);
                                if info.language == "text" {
                                    format = info.text_format(format)?;
                                }
                                renderer.set_format(format);
                            }
                        }
                        code_formats.push(info);
                    }
                    Tag::List(first_item_number) => {
                        lists.push(first_item_number);
                    }
                    Tag::Item => {
                        let item = lists.last_mut().expect("non-empty list list");
                        match *item {
                            Some(n) => {
                                let marker = format!("{:2}. ", n);
                                renderer.write(&marker)?;
                                renderer
                                    .set_format(renderer.format().with_added_indent(marker.len()));
                                *item.as_mut().unwrap() += 1;
                            }
                            None => {
                                renderer.write("  - ")?;
                                renderer.set_format(renderer.format().with_added_indent(4));
                            }
                        }
                    }
                    Tag::FootnoteDefinition(_s) => {}
                    Tag::Table(_alignments) => {}
                    Tag::TableHead => {}
                    Tag::TableRow => {}
                    Tag::TableCell => {}
                    Tag::Emphasis => {
                        renderer.set_format(renderer.format().with_flags(FormatFlags::UNDERLINE));
                    }
                    Tag::Strong => {
                        renderer.set_format(renderer.format().with_flags(FormatFlags::EMPHASIZED));
                    }
                    Tag::Strikethrough => {
                        renderer.set_format(renderer.format().with_strikethrough(true));
                    }
                    Tag::Link(_, _, _) => {}
                    Tag::Image(_, _, _) => {}
                }
            }
            Event::End(tag) => match tag {
                Tag::Paragraph => {
                    renderer.write("\n\n")?;
                }
                Tag::Heading(_, _, _) => {
                    // peel off everything but the centering command
                    renderer.restore_format();
                    renderer.write("\n\n")?;
                    // peel off the centering command now that we're at
                    // the start of a line
                    renderer.restore_format();
                }
                Tag::BlockQuote => {
                    renderer.restore_format();
                }
                Tag::CodeBlock(_) => match code_formats.pop().unwrap().language.as_ref() {
                    "bitmap" => {}
                    "image" => {}
                    "qrcode" => {}
                    "code128" => {}
                    _ => {
                        renderer.restore_format();
                    }
                },
                Tag::List(_first_item_number) => {
                    lists.pop();
                    renderer.write("\n")?;
                }
                Tag::Item => {
                    renderer.restore_format();
                    renderer.write("\n")?;
                }
                Tag::FootnoteDefinition(_s) => {}
                Tag::Table(_alignments) => {}
                Tag::TableHead => {}
                Tag::TableRow => {}
                Tag::TableCell => {}
                Tag::Emphasis => {
                    renderer.restore_format();
                }
                Tag::Strong => {
                    renderer.restore_format();
                }
                Tag::Strikethrough => {
                    renderer.restore_format();
                }
                Tag::Link(_, _, _) => {}
                Tag::Image(_, _, _) => {}
            },
            Event::Text(contents) => {
                match code_formats
                    .last()
                    .map(|i| i.language.as_ref())
                    .unwrap_or("")
                {
                    "bitmap" => {
                        write_bitmap(&mut renderer, contents.trim_end_matches('\n'))?;
                    }
                    "image" => {
                        write_image(&mut renderer, &contents)?;
                    }
                    "qrcode" => {
                        write_qrcode(&mut renderer, contents.trim())?;
                    }
                    "code128" => {
                        write_code128(&mut renderer, contents.trim())?;
                    }
                    _ => {
                        renderer.write(&contents)?;
                    }
                }
            }
            Event::Code(contents) => {
                renderer.set_format(renderer.format().with_red(true));
                renderer.write(&contents)?;
                renderer.restore_format();
            }
            Event::Html(_e) => {}
            Event::FootnoteReference(_e) => {}
            Event::SoftBreak => {
                renderer.write(" ")?;
            }
            Event::HardBreak => {
                renderer.write("\n\n")?;
            }
            Event::Rule => {
                renderer.cut();
            }
            Event::TaskListMarker(_checked) => {}
        }
    }

    renderer.cut();
    renderer.print()?;

    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
struct FormatInfo {
    language: String,
    options: Vec<String>,
}

impl FormatInfo {
    fn parse(info: &str) -> Self {
        let mut it = info.split_whitespace();
        Self {
            language: it.next().unwrap_or("").into(),
            options: it.map(|s| s.to_string()).collect(),
        }
    }

    fn text_format(&self, mut format: Rc<Format>) -> Result<Rc<Format>> {
        if self.language != "text" {
            bail!("language is not 'text'");
        }
        for option in &self.options {
            format = match option.as_ref() {
                "black" => format.with_red(false),
                "bold" => format.with_flags(FormatFlags::EMPHASIZED),
                _ => bail!("unknown option '{}'", option),
            }
        }
        Ok(format)
    }
}

fn write_bitmap(renderer: &mut Renderer<impl Read + Write>, contents: &str) -> Result<()> {
    let width = contents.split('\n').fold(0, |acc, l| acc.max(l.len()));
    let height = contents.split('\n').count();
    let mut image = GrayImage::new(
        width.try_into().context("invalid bitmap width")?,
        height.try_into().context("invalid bitmap height")?,
    );
    for pixel in image.pixels_mut() {
        pixel[0] = 255;
    }
    for (y, row) in contents.split('\n').enumerate() {
        for (x, value) in row.chars().enumerate() {
            image.get_pixel_mut(
                x.try_into().context("invalid X coordinate")?,
                y.try_into().context("invalid Y coordinate")?,
            )[0] = if value != ' ' { 0 } else { 255 };
        }
    }
    renderer.write_image(&image)
}

fn write_image(renderer: &mut Renderer<impl Read + Write>, contents: &str) -> Result<()> {
    let mut image = image::load_from_memory(contents.as_bytes())?.to_luma8();
    dither(&mut image, &BiLevel);
    renderer.write_image(&image)
}

fn write_qrcode(renderer: &mut Renderer<impl Read + Write>, contents: &str) -> Result<()> {
    // Build code
    let code = QrCode::with_error_correction_level(contents.as_bytes(), EcLevel::L)
        .context("creating QR code")?;
    // qrcode is supposed to be able to generate an Image directly,
    // but that doesn't work.  Take the long way around.
    // https://github.com/kennytm/qrcode-rust/issues/19
    let image_str_with_newlines = code
        .render()
        .module_dimensions(2, 2)
        .dark_color('#')
        .light_color(' ')
        .build();
    let image_str = image_str_with_newlines.replace('\n', "");
    let height = image_str_with_newlines.len() - image_str.len() + 1;
    let width = image_str.len() / height;
    let mut image = GrayImage::new(
        width.try_into().context("invalid QR code width")?,
        height.try_into().context("invalid QR code height")?,
    );
    for (item, pixel) in image_str.chars().zip(image.pixels_mut()) {
        pixel[0] = if item == '#' { 0 } else { 255 };
    }

    renderer.write_image(&image)
}

fn write_code128(renderer: &mut Renderer<impl Read + Write>, contents: &str) -> Result<()> {
    // Build code, character set B
    let data = Code128::new(format!("\u{0181}{}", contents))
        .context("creating barcode")?
        .encode();
    // The barcoders image feature pulls in all default features of `image`,
    // which are large.  Handle the conversion ourselves.
    let mut image = GrayImage::new(data.len().try_into().context("barcode size overflow")?, 24);
    for (x, value) in data.iter().enumerate() {
        for y in 0..image.height() {
            image.get_pixel_mut(x.try_into().context("invalid X coordinate")?, y)[0] =
                if *value > 0 { 0 } else { 255 };
        }
    }
    renderer.write_image(&image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clap() {
        use clap::CommandFactory;
        Args::command().debug_assert()
    }

    #[test]
    fn format_info_parse() {
        let tests = [
            (
                "",
                FormatInfo {
                    language: "".into(),
                    options: vec![],
                },
            ),
            (
                "foo",
                FormatInfo {
                    language: "foo".into(),
                    options: vec![],
                },
            ),
            (
                "  text	",
                FormatInfo {
                    language: "text".into(),
                    options: vec![],
                },
            ),
            (
                " text  black  bold ",
                FormatInfo {
                    language: "text".into(),
                    options: vec!["black".into(), "bold".into()],
                },
            ),
        ];
        for (info, expected) in tests {
            assert_eq!(FormatInfo::parse(info), expected);
        }
    }

    #[test]
    fn format_info_text_format() {
        let base = Format::new().with_red(true);

        let error = ["text bold blah", "foo bold"];
        for info in error {
            FormatInfo::parse(info)
                .text_format(base.clone())
                .unwrap_err();
        }

        let success = [
            ("text", base.clone()),
            ("text black", base.with_red(false)),
            (
                "text black bold",
                base.with_red(false).with_flags(FormatFlags::EMPHASIZED),
            ),
        ];
        for (info, expected) in success {
            assert_eq!(
                FormatInfo::parse(info).text_format(base.clone()).unwrap(),
                expected
            );
        }
    }
}
