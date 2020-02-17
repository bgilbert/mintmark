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

use barcoders::sym::code128::Code128;
use clap::{crate_version, App, Arg};
use image::GrayImage;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
use qrcode::{EcLevel, QrCode};
use std::convert::TryInto;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};

use render::{FormatFlags, Justification, Renderer};

fn main() -> Result<(), io::Error> {
    let args = App::new("mintmark")
        .version(crate_version!())
        .about("Print Markdown to an Epson TM-U220B receipt printer.")
        .arg(
            Arg::with_name("device")
                .value_name("DEVICE-PATH")
                .required(true)
                .help("path to the character device node"),
        )
        .get_matches();

    let mut input_bytes: Vec<u8> = Vec::new();
    io::stdin().lock().read_to_end(&mut input_bytes)?;
    let input = std::str::from_utf8(&input_bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let device = args.value_of("device").unwrap();
    let mut output = OpenOptions::new().read(true).write(true).open(device)?;

    render(&input, &mut output)
}

fn render<F: Read + Write>(input: &str, output: &mut F) -> Result<(), io::Error> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(input, options);

    let mut renderer = Renderer::new(output)?;
    let mut code_formats: Vec<String> = Vec::new();
    let mut lists: Vec<Option<u64>> = Vec::new();
    for (event, _) in parser.into_offset_iter() {
        match event {
            Event::Start(tag) => {
                match tag {
                    Tag::Paragraph => {}
                    Tag::Heading(size) => {
                        // Center first.  This only takes effect at the
                        // start of the line, so end tag handling needs to
                        // specially account for it.
                        renderer.set_format(
                            renderer.format().with_justification(Justification::Center),
                        );
                        match size {
                            1 => {
                                renderer.set_format(
                                    renderer.format().with_unidirectional(true).with_flags(
                                        FormatFlags::DOUBLE_HEIGHT
                                            | FormatFlags::DOUBLE_WIDTH
                                            | FormatFlags::EMPHASIZED
                                            | FormatFlags::UNDERLINE,
                                    ),
                                );
                            }
                            2 => {
                                renderer.set_format(
                                    renderer.format().with_unidirectional(true).with_flags(
                                        FormatFlags::DOUBLE_HEIGHT
                                            | FormatFlags::DOUBLE_WIDTH
                                            | FormatFlags::EMPHASIZED,
                                    ),
                                );
                            }
                            3 => {
                                renderer.set_format(
                                    renderer
                                        .format()
                                        .with_flags(
                                            FormatFlags::EMPHASIZED | FormatFlags::UNDERLINE,
                                        )
                                        .without_flags(FormatFlags::NARROW),
                                );
                            }
                            4 => {
                                renderer.set_format(
                                    renderer
                                        .format()
                                        .with_flags(FormatFlags::EMPHASIZED)
                                        .without_flags(FormatFlags::NARROW),
                                );
                            }
                            5 => {
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
                        let format = match kind {
                            CodeBlockKind::Indented => "".to_string(),
                            CodeBlockKind::Fenced(format_cow) => format_cow.into_string(),
                        };
                        match format.as_str() {
                            "image" => {}
                            "qrcode" => {}
                            "code128" => {}
                            _ => {
                                renderer.set_format(renderer.format().with_red(true));
                            }
                        }
                        code_formats.push(format);
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
                Tag::Heading(_) => {
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
                Tag::CodeBlock(kind) => {
                    code_formats.pop();
                    let format = match kind {
                        CodeBlockKind::Indented => "".to_string(),
                        CodeBlockKind::Fenced(format_cow) => format_cow.into_string(),
                    };
                    match format.as_str() {
                        "image" => {}
                        "qrcode" => {}
                        "code128" => {}
                        _ => {
                            renderer.restore_format();
                        }
                    }
                }
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
                match code_formats.last().unwrap_or(&"".to_string()).as_str() {
                    "image" => {
                        write_image(&mut renderer, &contents.trim_end_matches('\n'))?;
                    }
                    "qrcode" => {
                        write_qrcode(&mut renderer, &contents.trim())?;
                    }
                    "code128" => {
                        write_code128(&mut renderer, &contents.trim())?;
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
                renderer.cut()?;
            }
            Event::TaskListMarker(_checked) => {}
        }
    }

    renderer.cut()?;

    Ok(())
}

fn write_image<F: Read + Write>(
    renderer: &mut Renderer<F>,
    contents: &str,
) -> Result<(), io::Error> {
    let width = contents.split('\n').fold(0, |acc, l| acc.max(l.len()));
    let height = contents.split('\n').count();
    let mut image = GrayImage::new(
        width
            .try_into()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
        height
            .try_into()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
    );
    for pixel in image.pixels_mut() {
        pixel[0] = 255;
    }
    for (y, row) in contents.split('\n').enumerate() {
        for (x, value) in row.chars().enumerate() {
            image.get_pixel_mut(
                x.try_into()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
                y.try_into()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
            )[0] = if value != ' ' { 0 } else { 255 };
        }
    }
    renderer.write_image(&image)
}

fn write_qrcode<F: Read + Write>(
    renderer: &mut Renderer<F>,
    contents: &str,
) -> Result<(), io::Error> {
    // Build code
    let code = QrCode::with_error_correction_level(&contents.as_bytes(), EcLevel::L)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
    // qrcode is supposed to be able to generate an Image directly,
    // but that doesn't work.  Take the long way around.
    // https://github.com/kennytm/qrcode-rust/issues/19
    let image_str_with_newlines = code
        .render()
        .module_dimensions(2, 2)
        .dark_color('#')
        .light_color(' ')
        .build();
    let image_str = image_str_with_newlines.replace("\n", "");
    let height = image_str_with_newlines.len() - image_str.len() + 1;
    let width = image_str.len() / height;
    let mut image = GrayImage::new(
        width
            .try_into()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
        height
            .try_into()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
    );
    for (item, pixel) in image_str.chars().zip(image.pixels_mut()) {
        pixel[0] = if item == '#' { 0 } else { 255 };
    }

    renderer.write_image(&image)
}

fn write_code128<F: Read + Write>(
    renderer: &mut Renderer<F>,
    contents: &str,
) -> Result<(), io::Error> {
    // Build code, character set B
    let data = Code128::new(format!("\u{0181}{}", contents))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?
        .encode();
    // The barcoders image feature pulls in image format support, which is
    // large.  Handle the conversion ourselves.
    let mut image = GrayImage::new(
        data.len()
            .try_into()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
        24,
    );
    for (x, value) in data.iter().enumerate() {
        for y in 0..image.height() {
            image.get_pixel_mut(
                x.try_into()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
                y.try_into()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
            )[0] = if *value > 0 { 0 } else { 255 };
        }
    }
    renderer.write_image(&image)
}
