use bitflags::bitflags;
use encoding::all::ASCII;
use encoding::types::{EncoderTrap, Encoding};
use qrcode::{EcLevel, QrCode};
use std::convert::TryFrom;
use std::io::{self, Write};
use std::rc::Rc;

const LINE_PIXELS_IMAGE: usize = 200;
const LINE_PIXELS_TEXT: usize = 320;

bitflags! {
    pub struct RenderFlags: u8 {
        const NARROW = 0x01;
        const EMPHASIZED = 0x08;
        const DOUBLE_HEIGHT = 0x10;
        const DOUBLE_WIDTH = 0x20;
        const UNDERLINE = 0x80;
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Justification {
    Left = 0,
    Center = 1,
    #[allow(dead_code)]
    Right = 2,
}

#[derive(Clone)]
struct LineEntry {
    char: u8,
    state: Rc<RenderState>,
}

pub struct Renderer {
    state: Rc<RenderState>,
    stack: Vec<Rc<RenderState>>,

    line: Vec<LineEntry>,
    line_width: usize,

    word: Vec<LineEntry>,
    word_has_letters: bool,
}

#[derive(Clone, Eq, PartialEq)]
struct RenderState {
    flags: RenderFlags,
    line_spacing: u8,
    red: bool,
    unidirectional: bool,
    strikethrough: bool,
    justification: Justification,
}

impl Renderer {
    pub fn new() -> Result<Self, io::Error> {
        let state = Rc::new(RenderState {
            flags: RenderFlags::NARROW,
            line_spacing: 24,
            red: false,
            unidirectional: false,
            strikethrough: false,
            justification: Justification::Left,
        });
        let mut renderer = Renderer {
            state: state,
            stack: Vec::new(),
            line: Vec::new(),
            line_width: 0,
            word: Vec::new(),
            word_has_letters: false,
        };
        // Reset printer
        renderer.send(b"\x1b@")?;
        Ok(renderer)
    }

    fn new_state(&mut self) -> &mut RenderState {
        self.stack.push(self.state.clone());
        self.state = Rc::new((*self.state).clone());
        return Rc::get_mut(&mut self.state).unwrap();
    }

    pub fn set_flags(&mut self, flags: RenderFlags) -> Result<&mut Self, io::Error> {
        let state = self.new_state();
        state.flags |= flags;
        Ok(self)
    }

    pub fn clear_flags(&mut self, flags: RenderFlags) -> Result<&mut Self, io::Error> {
        let state = self.new_state();
        state.flags &= !flags;
        Ok(self)
    }

    pub fn set_line_spacing(&mut self, spacing: u8) -> Result<&mut Self, io::Error> {
        let state = self.new_state();
        state.line_spacing = spacing;
        Ok(self)
    }

    pub fn set_red(&mut self, red: bool) -> Result<&mut Self, io::Error> {
        let state = self.new_state();
        state.red = red;
        Ok(self)
    }

    pub fn set_unidirectional(&mut self, unidirectional: bool) -> Result<&mut Self, io::Error> {
        let state = self.new_state();
        state.unidirectional = unidirectional;
        Ok(self)
    }

    pub fn set_strikethrough(&mut self, strikethrough: bool) -> Result<&mut Self, io::Error> {
        let state = self.new_state();
        state.strikethrough = strikethrough;
        Ok(self)
    }

    pub fn set_justification(
        &mut self,
        justification: Justification,
    ) -> Result<&mut Self, io::Error> {
        let state = self.new_state();
        state.justification = justification;
        Ok(self)
    }

    pub fn restore(&mut self) -> Result<&mut Self, io::Error> {
        self.state = self
            .stack
            .pop()
            .expect("tried to unwind the root RenderState");
        Ok(self)
    }

    fn set_printer_state(&mut self, state: &RenderState) -> Result<(), io::Error> {
        self.send(b"\x1b!")?;
        self.send(&[state.flags.bits])?;
        self.send(b"\x1b3")?;
        self.send(&[state.line_spacing])?;
        self.send(b"\x1br")?;
        self.send(&[state.red as u8])?;
        self.send(b"\x1bU")?;
        self.send(&[state.unidirectional as u8])?;
        self.send(b"\x1ba")?;
        self.send(&[state.justification as u8])?;
        Ok(())
    }

    pub fn write(&mut self, contents: &str) -> Result<(), io::Error> {
        let mut bytes = ASCII
            .encode(contents, EncoderTrap::Replace)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        for byte in &mut bytes {
            // Got to the next word break?  Write out the word.
            if self.word_has_letters && (*byte == b'\n' || *byte == b' ') {
                // Start a new word.
                self.write_word()?;
            }
            // Hard line break?  Send it and move on.
            if *byte == b'\n' {
                self.send_line()?;
                continue;
            }
            // Map control sequences other than \n
            if *byte < 0x20 || *byte > 0x7e {
                *byte = b'?';
            }
            // Printables and spaces go in the word.  Once we have at
            // least one printable, the word becomes eligible for writing.
            self.word.push(LineEntry {
                char: *byte,
                state: self.state.clone(),
            });
            if *byte != b' ' {
                self.word_has_letters = true;
            }
        }
        Ok(())
    }

    fn write_word(&mut self) -> Result<(), io::Error> {
        let width = self
            .word
            .iter()
            .fold(0, |acc, entry| acc + entry.state.char_bounding_width());

        // If we have a partial line and this word won't fit on it, start
        // a new line.
        let mut soft_wrapped = false;
        if width <= LINE_PIXELS_TEXT && self.line_width + width > LINE_PIXELS_TEXT {
            self.send_line()?;
            soft_wrapped = true;
        }

        // Ignore spaces at the beginning of a soft-wrapped line, then
        // push the rest of the word.
        for entry in self
            .word
            .clone()
            .drain(..)
            .filter(|entry| !soft_wrapped || entry.char != b' ')
        {
            let char_width = entry.state.char_bounding_width();

            // If we've reached the end of the line just within this word,
            // just break in the middle of the word.
            if self.line_width + char_width > LINE_PIXELS_TEXT {
                self.send_line()?;
            }

            self.line.push(entry);
            self.line_width += char_width;
        }

        self.word.clear();
        self.word_has_letters = false;
        Ok(())
    }

    pub fn write_qr(&mut self, contents: &[u8]) -> Result<(), io::Error> {
        // Build code
        let code = QrCode::with_error_correction_level(contents, EcLevel::L)
            .expect("Building QR code failed");
        let image_str = code
            .render()
            .max_dimensions(LINE_PIXELS_IMAGE as u32, LINE_PIXELS_IMAGE as u32)
            .dark_color('#')
            .light_color(' ')
            .build();
        let mut image: Vec<Vec<bool>> = Vec::with_capacity(LINE_PIXELS_IMAGE);
        for line in image_str.split('\n') {
            let mut line_vec: Vec<bool> = Vec::with_capacity(LINE_PIXELS_IMAGE);
            for item in line.chars() {
                line_vec.push(item == '#');
            }
            image.push(line_vec);
        }
        let width = image[0].len();
        let height = image.len();

        // Flush line buffer if non-empty
        if self.line_width > 0 {
            self.send_line()?;
        }

        // Enable unidirectional print mode for better alignment
        self.set_unidirectional(true)?;
        // Set line spacing to avoid gaps
        self.set_line_spacing(16)?;
        // Center on line
        self.set_justification(Justification::Center)?;

        // Write code
        for yblock in 0..height / 8 {
            for byte in bit_image_prologue(width)? {
                self.line.push(LineEntry {
                    char: byte,
                    state: self.state.clone(),
                })
            }
            for x in 0..width {
                let mut byte: u8 = 0;
                for row in image.iter().skip(yblock * 8).take(8) {
                    byte <<= 1;
                    byte |= row[x] as u8;
                }
                self.line.push(LineEntry {
                    char: byte,
                    state: self.state.clone(),
                });
            }
            self.line_width += width;
            self.send_line()?;
        }

        // Restore print mode
        self.restore()?.restore()?.restore()?;

        Ok(())
    }

    // Advance paper and perform partial cut
    pub fn cut(&mut self) -> Result<(), io::Error> {
        self.send(b"\x1dV\x42\x68")
    }

    fn send_line(&mut self) -> Result<(), io::Error> {
        for pass in PASSES.iter() {
            if !self.active_for_line(pass) {
                continue;
            }
            // active_for_line() returned true, so there is at least one entry
            let mut state = self.line[0].state.clone();
            let mut active = (pass.active)(&state);
            self.set_printer_state(&(pass.state_map)((*state).clone(), active))?;
            for entry in self.line.clone().iter() {
                if *state != *entry.state {
                    state = entry.state.clone();
                    active = (pass.active)(&state);
                    self.set_printer_state(&(pass.state_map)((*state).clone(), active))?;
                }
                self.send(&(pass.char_map)(entry.char, &state, active))?;
            }
            self.send(b"\r")?;
        }
        self.send(b"\n")?;

        self.line.clear();
        self.line_width = 0;

        Ok(())
    }

    fn active_for_line(&self, pass: &LinePass) -> bool {
        for entry in self.line.iter() {
            if (pass.active)(&entry.state) {
                return true;
            }
        }
        false
    }

    fn send(&mut self, buf: &[u8]) -> Result<(), io::Error> {
        io::stdout().write_all(buf)
    }
}

impl RenderState {
    fn char_bounding_width(&self) -> usize {
        let mut width: usize = if !(self.flags & RenderFlags::NARROW).is_empty() {
            8
        } else {
            10
        };
        if !(self.flags & RenderFlags::DOUBLE_WIDTH).is_empty() {
            width *= 2
        }
        width
    }

    fn char_overstrike_width(&self) -> usize {
        let mut width: usize = if !(self.flags & RenderFlags::NARROW).is_empty() {
            5
        } else {
            6
        };
        if !(self.flags & RenderFlags::DOUBLE_WIDTH).is_empty() {
            width *= 2
        }
        width
    }
}

fn bit_image_prologue(width: usize) -> Result<Vec<u8>, io::Error> {
    match u16::try_from(width) {
        Ok(width_u16) => {
            let width_bytes = &width_u16.to_le_bytes();
            // Bit image mode 0, vert 72 dpi, horz 80 dpi, width 200 dots
            Ok(vec![0x1b, b'*', 0, width_bytes[0], width_bytes[1]])
        }
        Err(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "width too large",
        )),
    }
}

struct LinePass {
    #[allow(dead_code)]
    name: &'static str,
    active: fn(state: &RenderState) -> bool,
    state_map: fn(state: RenderState, active: bool) -> RenderState,
    char_map: fn(char: u8, state: &RenderState, active: bool) -> Vec<u8>,
}

fn strikethrough_char_map(_char: u8, state: &RenderState, active: bool) -> Vec<u8> {
    if active {
        let char_width = state.char_overstrike_width();
        let mut ret = bit_image_prologue(char_width).expect("overstrike width larger than u16");
        for _ in 0..char_width {
            ret.push(0x08);
        }
        ret
    } else {
        vec![b' ']
    }
}

static PASSES: [LinePass; 4] = [
    LinePass {
        name: "black",
        active: |state| !state.red,
        state_map: |mut state, active| {
            if !active {
                state.red = false;
                state.flags &= !RenderFlags::UNDERLINE
            };
            state
        },
        char_map: |char, _state, active| if active { vec![char] } else { vec![b' '] },
    },
    LinePass {
        name: "black strikethrough",
        active: |state| !state.red && state.strikethrough,
        state_map: |mut state, active| {
            if !active {
                state.red = false;
                state.flags &= !RenderFlags::UNDERLINE
            };
            state
        },
        char_map: strikethrough_char_map,
    },
    LinePass {
        name: "red",
        active: |state| state.red,
        state_map: |mut state, active| {
            if !active {
                state.red = true;
                state.flags &= !RenderFlags::UNDERLINE
            };
            state
        },
        char_map: |char, _state, active| if active { vec![char] } else { vec![b' '] },
    },
    LinePass {
        name: "red strikethrough",
        active: |state| state.red && state.strikethrough,
        state_map: |mut state, active| {
            if !active {
                state.red = true;
                state.flags &= !RenderFlags::UNDERLINE
            };
            state
        },
        char_map: strikethrough_char_map,
    },
];
