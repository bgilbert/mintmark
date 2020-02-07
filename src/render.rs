use bitflags::bitflags;
use encoding::all::ASCII;
use encoding::types::{EncoderTrap, Encoding};
use qrcode::{EcLevel, QrCode};
use std::io::{self, Write};

const LINE_PIXELS_IMAGE: u16 = 200;
const LINE_PIXELS_TEXT: u16 = 320;

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
enum LineEntry {
    Char(u8),
    State(RenderState),
}

pub struct Renderer {
    state: RenderState,
    stack: Vec<RenderState>,

    line: Vec<LineEntry>,
    line_start_state: RenderState,
    line_cur_state: RenderState,
    line_width: u16,
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
        let state = RenderState {
            flags: RenderFlags::NARROW,
            line_spacing: 24,
            red: false,
            unidirectional: false,
            strikethrough: false,
            justification: Justification::Left,
        };
        let mut renderer = Renderer {
            state: state.clone(),
            stack: Vec::new(),
            line: Vec::new(),
            line_start_state: state.clone(),
            line_cur_state: state,
            line_width: 0,
        };
        // Reset printer
        renderer.send(b"\x1b@")?;
        Ok(renderer)
    }

    pub fn set_flags(&mut self, flags: RenderFlags) -> Result<&mut Self, io::Error> {
        self.stack.push(self.state.clone());
        self.state.flags |= flags;
        self.set_state(&self.state.clone())?;
        Ok(self)
    }

    pub fn clear_flags(&mut self, flags: RenderFlags) -> Result<&mut Self, io::Error> {
        self.stack.push(self.state.clone());
        self.state.flags &= !flags;
        self.set_state(&self.state.clone())?;
        Ok(self)
    }

    pub fn set_line_spacing(&mut self, spacing: u8) -> Result<&mut Self, io::Error> {
        self.stack.push(self.state.clone());
        self.state.line_spacing = spacing;
        self.set_state(&self.state.clone())?;
        Ok(self)
    }

    pub fn set_red(&mut self, red: bool) -> Result<&mut Self, io::Error> {
        self.stack.push(self.state.clone());
        self.state.red = red;
        self.set_state(&self.state.clone())?;
        Ok(self)
    }

    pub fn set_unidirectional(&mut self, unidirectional: bool) -> Result<&mut Self, io::Error> {
        self.stack.push(self.state.clone());
        self.state.unidirectional = unidirectional;
        self.set_state(&self.state.clone())?;
        Ok(self)
    }

    pub fn set_strikethrough(&mut self, strikethrough: bool) -> Result<&mut Self, io::Error> {
        self.stack.push(self.state.clone());
        self.state.strikethrough = strikethrough;
        self.set_state(&self.state.clone())?;
        Ok(self)
    }

    pub fn set_justification(
        &mut self,
        justification: Justification,
    ) -> Result<&mut Self, io::Error> {
        self.stack.push(self.state.clone());
        self.state.justification = justification;
        self.set_state(&self.state.clone())?;
        Ok(self)
    }

    pub fn restore(&mut self) -> Result<&mut Self, io::Error> {
        self.state = self
            .stack
            .pop()
            .expect("tried to unwind the root RenderState");
        self.set_state(&self.state.clone())?;
        Ok(self)
    }

    fn set_state(&mut self, state: &RenderState) -> Result<(), io::Error> {
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
        if self.state != self.line_cur_state {
            self.line.push(LineEntry::State(self.state.clone()));
            self.line_cur_state = self.state.clone();
        }
        let mut bytes = ASCII
            .encode(contents, EncoderTrap::Replace)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        for byte in &mut bytes {
            if *byte == b'\n' {
                self.send_line()?;
                continue;
            }
            if *byte < 0x20 || *byte > 0x7e {
                *byte = b'?';
            }
            let char_width = self.state.char_bounding_width();
            if self.line_width + char_width > LINE_PIXELS_TEXT {
                self.send_line()?;
            }
            self.line.push(LineEntry::Char(*byte));
            self.line_width += char_width;
        }
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
        let mut image: Vec<Vec<bool>> = Vec::with_capacity(LINE_PIXELS_IMAGE as usize);
        for line in image_str.split('\n') {
            let mut line_vec: Vec<bool> = Vec::with_capacity(LINE_PIXELS_IMAGE as usize);
            for item in line.chars() {
                line_vec.push(item == '#');
            }
            image.push(line_vec);
        }
        let width = image[0].len();
        let height = image.len();

        // Enable unidirectional print mode for better alignment
        self.set_unidirectional(true)?;
        // Set line spacing to avoid gaps
        self.set_line_spacing(16)?;
        // Center on line
        self.set_justification(Justification::Center)?;

        // Write code
        for yblock in 0..height / 8 {
            let width_bytes = &(width as u16).to_le_bytes();
            // Bit image mode 0, vert 72 dpi, horz 80 dpi, width 200 dots
            let mut line: Vec<u8> = vec![0x1b, b'*', 0, width_bytes[0], width_bytes[1]];
            for x in 0..width {
                let mut byte: u8 = 0;
                for row in image.iter().skip(yblock * 8).take(8) {
                    byte <<= 1;
                    byte |= row[x] as u8;
                }
                line.push(byte);
            }
            self.send(&line)?;
            self.send(b"\n")?;
        }

        // Restore print mode
        self.restore()?.restore()?.restore()?;

        Ok(())
    }

    #[allow(dead_code)]
    pub fn rule(&mut self) -> Result<(), io::Error> {
        let width_bytes = &(LINE_PIXELS_IMAGE as u16).to_le_bytes();
        // Bit image mode 0, vert 72 dpi, horz 80 dpi, width 200 dots
        let mut line: Vec<u8> = vec![0x1b, b'*', 0, width_bytes[0], width_bytes[1]];
        line.resize(line.len() + LINE_PIXELS_IMAGE as usize, 0x10);
        line.push(b'\n');
        self.send(&line)?;
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
            let mut state = self.line_start_state.clone();
            let mut active = (pass.active)(&state);
            self.set_state(&(pass.state_map)(state.clone(), active))?;
            for entry in self.line.clone().iter() {
                match entry {
                    LineEntry::Char(c) => {
                        self.send(&(pass.char_map)(*c, &state, active))?;
                    }
                    LineEntry::State(new_state) => {
                        state = new_state.clone();
                        active = (pass.active)(&state);
                        self.set_state(&(pass.state_map)(state.clone(), active))?;
                    }
                }
            }
            self.send(b"\r")?;
        }
        self.send(b"\n")?;

        self.line.clear();
        self.line_start_state = self.state.clone();
        self.line_cur_state = self.state.clone();
        self.line_width = 0;

        Ok(())
    }

    fn active_for_line(&self, pass: &LinePass) -> bool {
        if (pass.active)(&self.line_start_state) {
            return true;
        }
        for entry in self.line.iter() {
            if let LineEntry::State(state) = entry {
                if (pass.active)(state) {
                    return true;
                }
            }
        }
        false
    }

    fn send(&mut self, buf: &[u8]) -> Result<(), io::Error> {
        io::stdout().write_all(buf)
    }
}

impl RenderState {
    fn char_bounding_width(&self) -> u16 {
        let mut width: u16 = if !(self.flags & RenderFlags::NARROW).is_empty() {
            8
        } else {
            10
        };
        if !(self.flags & RenderFlags::DOUBLE_WIDTH).is_empty() {
            width *= 2
        }
        width
    }
}

struct LinePass {
    #[allow(dead_code)]
    name: &'static str,
    active: fn(state: &RenderState) -> bool,
    state_map: fn(state: RenderState, active: bool) -> RenderState,
    char_map: fn(char: u8, state: &RenderState, active: bool) -> Vec<u8>,
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
        char_map: |_char, _state, active| if active { vec![b'-'] } else { vec![b' '] },
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
        char_map: |_char, _state, active| if active { vec![b'-'] } else { vec![b' '] },
    },
];
