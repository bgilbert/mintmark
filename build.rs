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

use anyhow::Result;
use std::env;
use std::fs::{read_dir, read_to_string, write};
use std::io::ErrorKind;

const HEIGHT: usize = 9;

fn main() -> Result<()> {
    custom_chars()
}

fn custom_chars() -> Result<()> {
    let mut out: Vec<u8> = Vec::new();
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
            let char = ent.file_name().to_string_lossy().as_bytes()[0];
            let contents = read_to_string(ent.path())?;
            let pixels = contents
                .split('\n')
                .map(|s| s.as_bytes())
                .collect::<Vec<&[u8]>>();

            // calculate character width
            let w = (0..max_width)
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

            // serialize character
            buf.extend(b"\x1b&\x02");
            buf.push(char);
            buf.push(char);
            buf.push(w as u8);
            for x in 0..w {
                let mut bits = 0u16;
                for y in 0..HEIGHT {
                    bits <<= 1;
                    bits |= (pixels
                        .get(y)
                        .copied()
                        .unwrap_or(&[] as &[u8])
                        .get(x)
                        .copied()
                        .unwrap_or(b' ')
                        != b' ') as u16;
                }
                bits <<= 16 - HEIGHT;
                buf.extend(bits.to_be_bytes());
            }
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
    write(
        format!("{}/custom.rs", env::var("OUT_DIR")?),
        format!("const CUSTOM_CHAR_INIT: [u8; {}] = {:?};\n", out.len(), out),
    )?;
    Ok(())
}
