mod render;

use barcoders::sym::code128::Code128;
use image::GrayImage;
use pulldown_cmark::{Event, Options, Parser, Tag};
use qrcode::{EcLevel, QrCode};
use std::convert::TryInto;
use std::io::{self, Read};

use render::{FormatFlags, Justification, Renderer, LINE_PIXELS_IMAGE};

fn main() -> Result<(), io::Error> {
    let mut input: Vec<u8> = Vec::new();
    io::stdin().lock().read_to_end(&mut input)?;

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(std::str::from_utf8(&input).expect("bad utf-8"), options);

    let mut renderer = Renderer::new()?;
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
                    Tag::CodeBlock(format_cow) => {
                        let format = format_cow.into_string();
                        match format.as_str() {
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
                Tag::CodeBlock(format) => {
                    code_formats.pop();
                    match format.into_string().as_str() {
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

fn write_qrcode(renderer: &mut Renderer, contents: &str) -> Result<(), io::Error> {
    // Build code
    let code = QrCode::with_error_correction_level(&contents.as_bytes(), EcLevel::L)
        .expect("Building QR code failed");
    // qrcode is supposed to be able to generate an Image directly,
    // but that doesn't work.  Take the long way around.
    // https://github.com/kennytm/qrcode-rust/issues/19
    let image_str_with_newlines = code
        .render()
        .max_dimensions(LINE_PIXELS_IMAGE as u32, LINE_PIXELS_IMAGE as u32)
        .dark_color('#')
        .light_color(' ')
        .build();
    let image_str = image_str_with_newlines.replace("\n", "");
    let height = image_str_with_newlines.len() - image_str.len() + 1;
    let width = image_str.len() / height;
    let mut image = GrayImage::new(
        width.try_into().expect("width too large"),
        height.try_into().expect("height too large"),
    );
    for (item, pixel) in image_str.chars().zip(image.pixels_mut()) {
        pixel[0] = if item == '#' { 255 } else { 0 };
    }

    renderer.write_image(&image)
}

fn write_code128(renderer: &mut Renderer, contents: &str) -> Result<(), io::Error> {
    // Build code, character set B
    let data = match Code128::new(format!("\u{0181}{}", contents)) {
        Ok(code) => code.encode(),
        Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidInput, e.to_string())),
    };
    // The barcoders image feature pulls in image format support, which is
    // large.  Handle the conversion ourselves.
    let mut image = GrayImage::new(data.len().try_into().expect("width too large"), 24);
    for (x, value) in data.iter().enumerate() {
        for y in 0..image.height() {
            image.get_pixel_mut(
                x.try_into().expect("width too large"),
                y.try_into().expect("height too large"),
            )[0] = if *value > 0 { 255 } else { 0 };
        }
    }
    renderer.write_image(&image)
}
