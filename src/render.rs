use bitflags::bitflags;
use encoding::all::ASCII;
use encoding::types::{EncoderTrap, Encoding};
use qrcode::{EcLevel, QrCode};
use std::io::{self, Write};

const LINE_PIXELS: u16 = 200;

bitflags! {
    pub struct RenderFlags: u8 {
        const NARROW = 0x01;
        const EMPHASIZED = 0x08;
        const DOUBLE_HEIGHT = 0x10;
        const DOUBLE_WIDTH = 0x20;
        const UNDERLINE = 0x80;
    }
}

#[derive(Copy, Clone)]
pub enum Justification {
    Left = 0,
    Center = 1,
    #[allow(dead_code)]
    Right = 2,
}

pub struct Renderer {
    state: RenderState,
    stack: Vec<RenderState>,
}

#[derive(Clone)]
struct RenderState {
    restore: Vec<u8>,

    flags: RenderFlags,
    line_spacing: u8,
    red: bool,
    unidirectional: bool,
    justification: Justification,
}

impl Renderer {
    pub fn new() -> Result<Self, io::Error> {
        let mut renderer = Renderer {
            state: RenderState {
                restore: Vec::new(),

                flags: RenderFlags::NARROW,
                line_spacing: 24,
                red: false,
                unidirectional: false,
                justification: Justification::Left,
            },
            stack: Vec::new(),
        };
        // Reset printer
        renderer.send(b"\x1b@")?;
        Ok(renderer)
    }

    fn save(&mut self) {
        self.stack.push(self.state.clone());
        self.state.restore = Vec::new();
    }

    // Returns reference to previous state.
    fn prev(&self) -> &RenderState {
        self.stack.last().expect("Root state has no parent")
    }

    fn mutate(&mut self, command: &[u8], old: &[u8], new: &[u8]) -> Result<&mut Self, io::Error> {
        self.send(command)?;
        self.send(new)?;
        self.state.restore.extend_from_slice(command);
        self.state.restore.extend_from_slice(old);
        Ok(self)
    }

    pub fn set_flags(&mut self, flags: RenderFlags) -> Result<&mut Self, io::Error> {
        self.save();
        self.state.flags |= flags;
        self.mutate(
            b"\x1b!",
            &[self.prev().flags.bits],
            &[self.state.flags.bits],
        )?;
        Ok(self)
    }

    pub fn clear_flags(&mut self, flags: RenderFlags) -> Result<&mut Self, io::Error> {
        self.save();
        self.state.flags &= !flags;
        self.mutate(
            b"\x1b!",
            &[self.prev().flags.bits],
            &[self.state.flags.bits],
        )?;
        Ok(self)
    }

    pub fn set_line_spacing(&mut self, spacing: u8) -> Result<&mut Self, io::Error> {
        self.save();
        self.state.line_spacing = spacing;
        self.mutate(
            b"\x1b3",
            &[self.prev().line_spacing],
            &[self.state.line_spacing],
        )?;
        Ok(self)
    }

    pub fn set_red(&mut self, red: bool) -> Result<&mut Self, io::Error> {
        self.save();
        self.state.red = red;
        self.mutate(
            b"\x1br",
            &[self.prev().red as u8],
            &[self.state.red as u8],
        )?;
        Ok(self)
    }

    pub fn set_unidirectional(&mut self, unidirectional: bool) -> Result<&mut Self, io::Error> {
        self.save();
        self.state.unidirectional = unidirectional;
        self.mutate(
            b"\x1bU",
            &[self.prev().unidirectional as u8],
            &[self.state.unidirectional as u8],
        )?;
        Ok(self)
    }

    pub fn set_justification(
        &mut self,
        justification: Justification,
    ) -> Result<&mut Self, io::Error> {
        self.save();
        self.state.justification = justification;
        self.mutate(
            b"\x1ba",
            &[self.prev().justification as u8],
            &[self.state.justification as u8],
        )?;
        Ok(self)
    }

    pub fn restore(&mut self) -> Result<&mut Self, io::Error> {
        self.send(&self.state.restore.clone())?;
        self.state = self
            .stack
            .pop()
            .expect("tried to unwind the root RenderState");
        Ok(self)
    }

    pub fn write(&mut self, contents: &str) -> Result<(), io::Error> {
        let mut bytes = ASCII
            .encode(contents, EncoderTrap::Replace)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        for byte in &mut bytes {
            if (*byte < 0x20 || *byte > 0x7e) && *byte != b'\n' {
                *byte = b'?';
            }
        }
        self.send(&bytes)
    }

    pub fn write_qr(&mut self, contents: &[u8]) -> Result<(), io::Error> {
        // Build code
        let code = QrCode::with_error_correction_level(contents, EcLevel::L)
            .expect("Building QR code failed");
        let image_str = code
            .render()
            .max_dimensions(LINE_PIXELS as u32, LINE_PIXELS as u32)
            .dark_color('#')
            .light_color(' ')
            .build();
        let mut image: Vec<Vec<bool>> = Vec::with_capacity(LINE_PIXELS as usize);
        for line in image_str.split('\n') {
            let mut line_vec: Vec<bool> = Vec::with_capacity(LINE_PIXELS as usize);
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
        let width_bytes = &(LINE_PIXELS as u16).to_le_bytes();
        // Bit image mode 0, vert 72 dpi, horz 80 dpi, width 200 dots
        let mut line: Vec<u8> = vec![0x1b, b'*', 0, width_bytes[0], width_bytes[1]];
        line.resize(line.len() + LINE_PIXELS as usize, 0x10);
        line.push(b'\n');
        self.send(&line)?;
        Ok(())
    }

    // Advance paper and perform partial cut
    pub fn cut(&mut self) -> Result<(), io::Error> {
        self.send(b"\x1dV\x42\x68")
    }

    fn send(&mut self, buf: &[u8]) -> Result<(), io::Error> {
        io::stdout().write_all(buf)
    }
}
