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

use anyhow::{bail, Result};
use std::env;
use std::fs::{read_dir, read_to_string, write};
use std::io::ErrorKind;

const HEIGHT: usize = 9;
const MAX_CHARS: u32 = 20;

fn main() -> Result<()> {
    custom_chars()
}

fn custom_chars() -> Result<()> {
    let mut out: Vec<u8> = Vec::new();
    let mut count = 0;
    for (font_name, font_num, max_width) in [("wide", 0, 12), ("narrow", 1, 10)] {
        let dir_path = format!("src/custom/{font_name}");
        println!("cargo:rerun-if-changed={}", dir_path);

        let dir_iter = match read_dir(dir_path) {
            Ok(it) => it,
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => Err(e)?,
        };

        let mut buf = Vec::new();
        for ent in dir_iter {
            let ent = ent?;
            println!("cargo:rerun-if-changed={}", ent.path().display());

            // read pixels from file
            let filename_bytes = ent.file_name().to_string_lossy().as_bytes().to_vec();
            if filename_bytes.len() != 1 {
                bail!("Multi-character filename: {}", ent.path().display());
            }
            let char = filename_bytes[0];
            if !(0x20..=0x7e).contains(&char) {
                bail!("{font_name} character outside valid range: {}", char);
            }
            let contents = read_to_string(ent.path())?;
            let pixels = contents
                .trim_end()
                .split('\n')
                .map(|s| s.as_bytes())
                .collect::<Vec<&[u8]>>();
            if pixels.len() > HEIGHT {
                bail!(
                    "Character in {} too tall: {} > {HEIGHT}",
                    ent.path().display(),
                    pixels.len()
                )
            }

            // calculate character width
            let w = (0..max_width + 1)
                .filter(|x| {
                    (0..HEIGHT).any(|y| {
                        pixels
                            .get(y)
                            .copied()
                            .unwrap_or(&[] as &[u8])
                            .get(*x)
                            .copied()
                            .unwrap_or(b' ')
                            != b' '
                    })
                })
                .max()
                .map(|x| x + 1)
                .unwrap_or(0);
            if w > max_width {
                bail!(
                    "Character in {} wider than {max_width}",
                    ent.path().display()
                );
            }

            // serialize character
            buf.extend(b"\x1b&\x02");
            buf.push(char);
            buf.push(char);
            buf.push(w as u8);
            let mut prev = 0;
            for x in 0..w {
                let mut bits = 0u16;
                for y in 0..HEIGHT {
                    bits <<= 1;
                    let cur_bit = pixels
                        .get(y)
                        .copied()
                        .unwrap_or(&[] as &[u8])
                        .get(x)
                        .copied()
                        .unwrap_or(b' ')
                        != b' ';
                    let prev_bit = prev & 0x8000 != 0;
                    // verify the second half of a dot is marked as set, then
                    // swallow it
                    if !prev_bit && cur_bit {
                        // first half of a dot; record it
                        bits |= 1;
                    } else if prev_bit && !cur_bit {
                        // missing second half
                        bail!("Found a dot not two columns wide: {}", ent.path().display());
                    }
                    prev <<= 1;
                }
                bits <<= 16 - HEIGHT;
                buf.extend(bits.to_be_bytes());
                prev = bits;
            }
            count += 1;
        }

        // set font and store custom chars if we have any
        if !buf.is_empty() {
            out.extend(b"\x1bM");
            out.push(font_num);
            out.append(&mut buf);
        }
    }

    // enable custom fonts if we have custom chars
    if !out.is_empty() {
        out.extend(b"\x1b%\x01");
    }

    // write output
    if count > MAX_CHARS {
        bail!("too many custom characters: {count} > {MAX_CHARS}");
    }
    write(
        format!("{}/custom.rs", env::var("OUT_DIR")?),
        format!("const CUSTOM_CHAR_INIT: [u8; {}] = {:?};\n", out.len(), out),
    )?;
    Ok(())
}
