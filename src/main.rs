/*
 * Copyright 2020-2022 Benjamin Gilbert
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

mod codeblock;
mod render;
mod strike;

use anyhow::{Context, Result};
use clap::Parser as ClapParser;
use fs2::FileExt;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;

use codeblock::CodeBlockConfig;
use render::{FormatFlags, Justification, Renderer};

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
    let mut code_block: Option<CodeBlockConfig> = None;
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
                        assert!(code_block.is_none());
                        code_block = Some(CodeBlockConfig::from_info(&info)?);
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
                Tag::CodeBlock(_) => {
                    assert!(code_block.is_some());
                    code_block = None;
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
                if let Some(block) = code_block.as_ref() {
                    block.render(&mut renderer, &contents)?;
                } else {
                    renderer.write(&contents)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clap() {
        use clap::CommandFactory;
        Args::command().debug_assert()
    }
}
