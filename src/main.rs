use std::io::{self, Write};

use qrcode::{EcLevel, QrCode};

const LINE_PIXELS: u16 = 200;

fn send(buf: &[u8]) -> Result<(), io::Error> {
    io::stdout().write_all(buf)
}

fn print_qr(contents: &[u8]) -> Result<(), io::Error> {
    // Build code
    let code =
        QrCode::with_error_correction_level(contents, EcLevel::L).expect("Building QR code failed");
    let image_str = code
        .render()
        .max_dimensions(LINE_PIXELS as u32, LINE_PIXELS as u32)
        .dark_color('#')
        .light_color(' ')
        .build();
    let mut image: Vec<Vec<bool>> = Vec::with_capacity(LINE_PIXELS as usize);
    for line in image_str.split("\n") {
        let mut line_vec: Vec<bool> = Vec::with_capacity(LINE_PIXELS as usize);
        let pad_size = (LINE_PIXELS as usize - line.len()) / 2;
        for _ in 0..pad_size {
            line_vec.push(false);
        }
        for item in line.chars() {
            line_vec.push(item == '#');
        }
        image.push(line_vec);
    }
    let width = image[0].len();
    let height = image.len();

    // Enable unidirectional print mode for better alignment
    send(b"\x1bU\x01")?;
    // Set line spacing to avoid gaps
    send(b"\x1b3\x0e")?;

    // Write code
    for yblock in 0..height / 8 {
        let width_bytes = &(width as u16).to_le_bytes();
        // Bit image mode 0, vert 72 dpi, horz 80 dpi, width 200 dots
        let mut line: Vec<u8> = vec![0x1b, b'*', 0, width_bytes[0], width_bytes[1]];
        for x in 0..width {
            let mut byte: u8 = 0;
            for y in yblock * 8..yblock * 8 + 7 {
                byte <<= 1;
                byte |= image[y][x] as u8;
            }
            line.push(byte);
        }
        send(&line)?;
        send(b"\r")?;
        send(&line)?;
        send(b"\n")?;
    }

    // Restore bidirectional print mode
    send(b"\x1bU\x00")?;
    // Restore line spacing
    send(b"\x1b2")?;

    Ok(())
}

fn main() -> Result<(), io::Error> {
    // Reset printer
    send(b"\x1b@")?;

    // Print QR code
    print_qr(b"THIS CERTIFICATE GOOD FOR ONE AWESOME")?;

    // Advance paper and cut
    send(b"\x1dV\x42\x68")?;

    Ok(())
}
