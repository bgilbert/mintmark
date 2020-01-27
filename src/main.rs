mod render;

use pulldown_cmark::{Event, Options, Parser, Tag};
use std::io::{self, Read};

use render::{Justification, RenderFlags, Renderer};

fn main() -> Result<(), io::Error> {
    let mut input: Vec<u8> = Vec::new();
    io::stdin().lock().read_to_end(&mut input)?;

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(std::str::from_utf8(&input).expect("bad utf-8"), options);

    let mut renderer = Renderer::new()?;
    let mut in_qr_code: u32 = 0;
    for (event, _) in parser.into_offset_iter() {
        match event {
            Event::Start(tag) => {
                match tag {
                    Tag::Paragraph => {}
                    Tag::Heading(size) => {
                        // Center first.  This only takes effect at the
                        // start of the line, so end tag handling needs to
                        // specially account for it.
                        renderer.set_justification(Justification::Center)?;
                        match size {
                            1 => {
                                renderer.set_unidirectional(true)?.set_flags(
                                    RenderFlags::DOUBLE_HEIGHT
                                        | RenderFlags::DOUBLE_WIDTH
                                        | RenderFlags::EMPHASIZED
                                        | RenderFlags::UNDERLINE,
                                )?;
                            }
                            2 => {
                                renderer.set_unidirectional(true)?.set_flags(
                                    RenderFlags::DOUBLE_HEIGHT
                                        | RenderFlags::DOUBLE_WIDTH
                                        | RenderFlags::EMPHASIZED,
                                )?;
                            }
                            3 => {
                                renderer
                                    .set_flags(RenderFlags::EMPHASIZED | RenderFlags::UNDERLINE)?
                                    .clear_flags(RenderFlags::NARROW)?;
                            }
                            4 => {
                                renderer
                                    .set_flags(RenderFlags::EMPHASIZED)?
                                    .clear_flags(RenderFlags::NARROW)?;
                            }
                            5 => {
                                renderer
                                    .set_flags(RenderFlags::EMPHASIZED | RenderFlags::UNDERLINE)?;
                            }
                            _ => {
                                renderer.set_flags(RenderFlags::EMPHASIZED)?;
                            }
                        }
                    }
                    Tag::BlockQuote => {}
                    Tag::CodeBlock(format) => match format.into_string().as_str() {
                        "qr" => {
                            in_qr_code += 1;
                        }
                        _ => {
                            renderer.set_red(true)?;
                        }
                    },
                    Tag::List(_first_item_number) => {}
                    Tag::Item => {
                        renderer.write("  - ")?;
                    }
                    Tag::FootnoteDefinition(_s) => {}
                    Tag::Table(_alignments) => {}
                    Tag::TableHead => {}
                    Tag::TableRow => {}
                    Tag::TableCell => {}
                    Tag::Emphasis => {
                        renderer.set_flags(RenderFlags::UNDERLINE)?;
                    }
                    Tag::Strong => {
                        renderer.set_flags(RenderFlags::EMPHASIZED)?;
                    }
                    Tag::Strikethrough => {}
                    Tag::Link(_, _, _) => {}
                    Tag::Image(_, _, _) => {}
                }
            }
            Event::End(tag) => match tag {
                Tag::Paragraph => {
                    renderer.write("\n\n")?;
                }
                Tag::Heading(size) => {
                    // peel off everything but the centering command
                    match size {
                        1 | 2 | 3 | 4 => {
                            renderer.restore()?.restore()?;
                        }
                        5 | _ => {
                            renderer.restore()?;
                        }
                    }
                    renderer.write("\n\n")?;
                    // peel off the centering command now that we're at
                    // the start of a line
                    renderer.restore()?;
                }
                Tag::BlockQuote => {}
                Tag::CodeBlock(format) => match format.into_string().as_str() {
                    "qr" => {
                        in_qr_code -= 1;
                    }
                    _ => {
                        renderer.restore()?;
                    }
                },
                Tag::List(_first_item_number) => {
                    renderer.write("\n")?;
                }
                Tag::Item => {
                    renderer.write("\n")?;
                }
                Tag::FootnoteDefinition(_s) => {}
                Tag::Table(_alignments) => {}
                Tag::TableHead => {}
                Tag::TableRow => {}
                Tag::TableCell => {}
                Tag::Emphasis => {
                    renderer.restore()?;
                }
                Tag::Strong => {
                    renderer.restore()?;
                }
                Tag::Strikethrough => {}
                Tag::Link(_, _, _) => {}
                Tag::Image(_, _, _) => {}
            },
            Event::Text(contents) => {
                if in_qr_code > 0 {
                    renderer.write_qr(&contents.as_bytes())?;
                } else {
                    renderer.write(&contents)?;
                }
            }
            Event::Code(contents) => {
                renderer.set_red(true)?;
                renderer.write(&contents)?;
                renderer.restore()?;
            }
            Event::Html(_e) => {}
            Event::FootnoteReference(_e) => {}
            Event::SoftBreak => {
                renderer.write("\n")?;
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
