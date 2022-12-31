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

use anyhow::{anyhow, bail, Context, Result};
use bitflags::bitflags;
use encoding::all::ASCII;
use encoding::types::{EncoderTrap, Encoding};
use image::{GrayImage, Luma};
use std::io::{Read, Write};
use std::rc::Rc;

const LINE_PIXELS_IMAGE: usize = 200;
const LINE_PIXELS_TEXT: usize = 320;

pub struct Renderer<F: Read + Write> {
    device: F,
    buf: Vec<u8>,

    format: Rc<Format>,
    stack: Vec<Rc<Format>>,

    line: Vec<LineChar>,
    line_width: usize,

    word: Vec<LineChar>,
    word_has_letters: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Format {
    flags: FormatFlags,
    line_spacing: u8,
    indent: usize,
    red: bool,
    unidirectional: bool,
    strikethrough: bool,
    justification: Justification,
}

bitflags! {
    pub struct FormatFlags: u8 {
        const NARROW = 0x01;
        const EMPHASIZED = 0x08;
        const DOUBLE_HEIGHT = 0x10;
        const DOUBLE_WIDTH = 0x20;
        const UNDERLINE = 0x80;
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Justification {
    Left = 0,
    Center = 1,
    #[allow(dead_code)]
    Right = 2,
}

#[derive(Clone)]
struct LineChar {
    char: u8,
    format: Rc<Format>,
}

impl<F: Read + Write> Renderer<F> {
    pub fn new(device: F) -> Self {
        let mut renderer = Renderer::<F> {
            device,
            buf: Vec::new(),
            format: Format::new(),
            stack: Vec::new(),
            line: Vec::new(),
            line_width: 0,
            word: Vec::new(),
            word_has_letters: false,
        };
        // Reset printer
        renderer.spool(b"\x1b@");
        renderer
    }

    pub fn format(&self) -> Rc<Format> {
        self.format.clone()
    }

    pub fn set_format(&mut self, format: Rc<Format>) {
        self.stack.push(self.format.clone());
        self.format = format;
    }

    pub fn restore_format(&mut self) {
        self.format = self.stack.pop().expect("tried to unwind the root Format");
    }

    fn set_printer_format(&mut self, format: &Format) {
        self.spool(b"\x1b!");
        self.spool(&[format.flags.bits]);
        self.spool(b"\x1b3");
        self.spool(&[format.line_spacing]);
        self.spool(b"\x1br");
        self.spool(&[format.red as u8]);
        self.spool(b"\x1bU");
        self.spool(&[format.unidirectional as u8]);
        self.spool(b"\x1ba");
        self.spool(&[format.justification as u8]);
    }

    pub fn write(&mut self, contents: &str) -> Result<()> {
        let mut bytes = ASCII
            .encode(contents, EncoderTrap::Replace)
            .map_err(|e| anyhow!(e).context("encoding text"))?;
        for byte in &mut bytes {
            // Got to the next word break?  Write out the word.
            if self.word_has_letters && (*byte == b'\n' || *byte == b' ') {
                // Start a new word.
                self.write_word();
            }
            // Hard line break?  Send it and move on.
            if *byte == b'\n' {
                self.spool_line();
                continue;
            }
            // Map other control sequences other than \t
            if (*byte < 0x20 || *byte > 0x7e) && *byte != b'\t' {
                *byte = b'?';
            }
            // Printables and spaces go in the word.  Once we have at
            // least one printable, the word becomes eligible for writing.
            self.word.push(LineChar {
                char: *byte,
                format: self.format.clone(),
            });
            if *byte != b' ' {
                self.word_has_letters = true;
            }
        }
        Ok(())
    }

    fn write_word(&mut self) {
        let width = self
            .word
            .iter()
            .fold(0, |acc, lc| acc + lc.format.char_bounding_width());

        // If we have a partial line and this word won't fit on it, start
        // a new line.
        let soft_wrapped =
            if width <= LINE_PIXELS_TEXT && self.line_width + width > LINE_PIXELS_TEXT {
                self.spool_line();
                true
            } else {
                false
            };

        // Ignore spaces at the beginning of a soft-wrapped line, then
        // push the rest of the word.
        for lc in self
            .word
            .clone()
            .drain(..)
            .filter(|lc| !soft_wrapped || lc.char != b' ')
        {
            let char_width = lc.format.char_bounding_width();

            // If we've reached the end of the line just within this word,
            // just break in the middle of the word.
            if self.line_width + char_width > LINE_PIXELS_TEXT {
                self.spool_line();
            }

            // Add indent if at the beginning of the line
            if self.line_width == 0 {
                for _ in 0..lc.format.indent {
                    self.line.push(LineChar {
                        char: b' ',
                        format: lc.format.clone(),
                    })
                }
                self.line_width += lc.format.indent * char_width;
            }

            self.line.push(lc);
            self.line_width += char_width;
        }

        self.word.clear();
        self.word_has_letters = false;
    }

    pub fn write_image(&mut self, image: &GrayImage) -> Result<()> {
        if image.width() as usize > LINE_PIXELS_IMAGE {
            bail!(
                "Image width {} larger than maximum {}",
                image.width(),
                LINE_PIXELS_IMAGE
            );
        }

        // Flush line buffer if non-empty
        if self.line_width > 0 {
            self.spool_line();
        }

        self.set_format(
            self.format()
                // Enable unidirectional print mode for better alignment
                .with_unidirectional(true)
                // Set line spacing to avoid gaps
                .with_line_spacing(16)
                // Center on line
                .with_justification(Justification::Center),
        );

        // Write image
        for yblock in 0..(image.height() + 7) / 8 {
            for byte in bit_image_prologue(image.width() as usize)? {
                self.line.push(LineChar {
                    char: byte,
                    format: self.format.clone(),
                })
            }
            for x in 0..image.width() {
                let mut byte: u8 = 0;
                for y in yblock * 8..(yblock + 1) * 8 {
                    let Luma(level) = if y < image.height() {
                        image.get_pixel(x, y)
                    } else {
                        &Luma([255])
                    };
                    byte <<= 1;
                    byte |= (level[0] < 128) as u8;
                }
                self.line.push(LineChar {
                    char: byte,
                    format: self.format.clone(),
                });
            }
            self.line_width += image.width() as usize;
            self.spool_line();
        }

        // Restore print mode
        self.restore_format();

        Ok(())
    }

    // Advance paper and perform partial cut
    pub fn cut(&mut self) {
        // Flush line buffer if non-empty
        if self.line_width > 0 {
            self.spool_line();
        }

        self.spool(b"\x1dV\x42\x50")
    }

    fn spool_line(&mut self) {
        for pass in PASSES.iter() {
            if !self.active_for_line(pass) {
                continue;
            }
            // active_for_line() returned true, so there is at least one LineChar
            let mut format = self.line[0].format.clone();
            let mut active = (pass.active)(&format);
            self.set_printer_format(&(pass.format_map)((*format).clone(), active));
            for lc in self.line.clone().iter() {
                if *format != *lc.format {
                    format = lc.format.clone();
                    active = (pass.active)(&format);
                    self.set_printer_format(&(pass.format_map)((*format).clone(), active));
                }
                self.spool(&(pass.char_map)(lc.char, &format, active));
            }
            self.spool(b"\r");
        }
        self.spool(b"\n");

        self.line.clear();
        self.line_width = 0;
    }

    fn active_for_line(&self, pass: &LinePass) -> bool {
        self.line.iter().any(|lc| (pass.active)(&lc.format))
    }

    fn spool(&mut self, buf: &[u8]) {
        self.buf.extend_from_slice(buf);
    }

    pub fn print(&mut self) -> Result<()> {
        self.device
            .write_all(&self.buf)
            .context("writing to device")?;
        self.buf.clear();
        Ok(())
    }
}

impl Format {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            flags: FormatFlags::NARROW,
            line_spacing: 24,
            indent: 0,
            red: false,
            unidirectional: false,
            strikethrough: false,
            justification: Justification::Left,
        })
    }

    pub fn with_flags(&self, flags: FormatFlags) -> Rc<Self> {
        let mut format = self.clone();
        format.flags |= flags;
        Rc::new(format)
    }

    pub fn without_flags(&self, flags: FormatFlags) -> Rc<Self> {
        let mut format = self.clone();
        format.flags &= !flags;
        Rc::new(format)
    }

    pub fn with_line_spacing(&self, spacing: u8) -> Rc<Self> {
        let mut format = self.clone();
        format.line_spacing = spacing;
        Rc::new(format)
    }

    pub fn with_added_indent(&self, indent: usize) -> Rc<Self> {
        let mut format = self.clone();
        format.indent += indent;
        Rc::new(format)
    }

    pub fn with_red(&self, red: bool) -> Rc<Self> {
        let mut format = self.clone();
        format.red = red;
        Rc::new(format)
    }

    pub fn with_unidirectional(&self, unidirectional: bool) -> Rc<Self> {
        let mut format = self.clone();
        format.unidirectional = unidirectional;
        Rc::new(format)
    }

    pub fn with_strikethrough(&self, strikethrough: bool) -> Rc<Self> {
        let mut format = self.clone();
        format.strikethrough = strikethrough;
        Rc::new(format)
    }

    pub fn with_justification(&self, justification: Justification) -> Rc<Self> {
        let mut format = self.clone();
        format.justification = justification;
        Rc::new(format)
    }

    fn char_bounding_width(&self) -> usize {
        let mut width: usize = if !(self.flags & FormatFlags::NARROW).is_empty() {
            8
        } else {
            10
        };
        if !(self.flags & FormatFlags::DOUBLE_WIDTH).is_empty() {
            width *= 2
        }
        width
    }

    fn char_overstrike_width(&self) -> usize {
        let mut width: usize = if !(self.flags & FormatFlags::NARROW).is_empty() {
            5
        } else {
            6
        };
        if !(self.flags & FormatFlags::DOUBLE_WIDTH).is_empty() {
            width *= 2
        }
        width
    }
}

fn bit_image_prologue(width: usize) -> Result<Vec<u8>> {
    let width_u16 = u16::try_from(width).context("bit image width too large")?;
    let width_bytes = &width_u16.to_le_bytes();
    // Bit image mode 0, vert 72 dpi, horz 80 dpi, width 200 dots
    Ok(vec![0x1b, b'*', 0, width_bytes[0], width_bytes[1]])
}

struct LinePass {
    #[allow(dead_code)]
    name: &'static str,
    active: fn(format: &Format) -> bool,
    format_map: fn(format: Format, active: bool) -> Format,
    char_map: fn(char: u8, format: &Format, active: bool) -> Vec<u8>,
}

fn strikethrough_char_map(_char: u8, format: &Format, active: bool) -> Vec<u8> {
    if active {
        let char_width = format.char_overstrike_width();
        let mut ret = bit_image_prologue(char_width).expect("overstrike width larger than u16");
        ret.resize(ret.len() + char_width, 0x08);
        ret
    } else {
        vec![b' ']
    }
}

static PASSES: [LinePass; 4] = [
    LinePass {
        name: "black",
        active: |format| !format.red,
        format_map: |mut format, active| {
            if !active {
                format.red = false;
                format.flags &= !FormatFlags::UNDERLINE
            };
            format
        },
        char_map: |char, _format, active| if active { vec![char] } else { vec![b' '] },
    },
    LinePass {
        name: "black strikethrough",
        active: |format| !format.red && format.strikethrough,
        format_map: |mut format, active| {
            if !active {
                format.red = false;
                format.flags &= !FormatFlags::UNDERLINE
            };
            format
        },
        char_map: strikethrough_char_map,
    },
    LinePass {
        name: "red",
        active: |format| format.red,
        format_map: |mut format, active| {
            if !active {
                format.red = true;
                format.flags &= !FormatFlags::UNDERLINE
            };
            format
        },
        char_map: |char, _format, active| if active { vec![char] } else { vec![b' '] },
    },
    LinePass {
        name: "red strikethrough",
        active: |format| format.red && format.strikethrough,
        format_map: |mut format, active| {
            if !active {
                format.red = true;
                format.flags &= !FormatFlags::UNDERLINE
            };
            format
        },
        char_map: strikethrough_char_map,
    },
];
