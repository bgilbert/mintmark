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

use anyhow::{bail, Context, Result};
use barcoders::sym::code128::Code128;
use image::imageops::colorops::{dither, ColorMap};
use image::{Rgb, RgbImage};
use qrcode::{EcLevel, QrCode};
use std::borrow::Cow;
use std::io::{Read, Write};

use crate::render::Renderer;
use crate::FormatInfo;

pub(crate) fn write_bitmap(
    renderer: &mut Renderer<impl Read + Write>,
    contents: &str,
) -> Result<()> {
    let width = contents.split('\n').fold(0, |acc, l| acc.max(l.len()));
    let height = contents.split('\n').count();
    let mut image = RgbImage::new(
        width.try_into().context("invalid bitmap width")?,
        height.try_into().context("invalid bitmap height")?,
    );
    for pixel in image.pixels_mut() {
        *pixel = Colors::COLOR_WHITE;
    }
    for (y, row) in contents.split('\n').enumerate() {
        for (x, value) in row.chars().enumerate() {
            *image.get_pixel_mut(
                x.try_into().context("invalid X coordinate")?,
                y.try_into().context("invalid Y coordinate")?,
            ) = if value != ' ' {
                Colors::COLOR_BLACK
            } else {
                Colors::COLOR_WHITE
            };
        }
    }
    renderer.write_image(&image)
}

pub(crate) fn write_image(
    renderer: &mut Renderer<impl Read + Write>,
    info: &FormatInfo,
    contents: &str,
) -> Result<()> {
    assert!(info.language == "image");
    let mut base64 = false;
    let mut bicolor = false;
    for option in &info.options {
        match option.as_ref() {
            "base64" => base64 = true,
            "bicolor" => bicolor = true,
            _ => bail!("unknown option '{}'", option),
        }
    }

    let data = if base64 {
        Cow::from(base64::decode(contents.replace(['\r', '\n'], "")).context("decoding base64")?)
    } else {
        Cow::from(contents.as_bytes())
    };
    let mut image = image::load_from_memory(&data)?.to_rgb8();
    dither(&mut image, &Colors::new(bicolor));
    renderer.write_image(&image)
}

pub(crate) fn write_qrcode(
    renderer: &mut Renderer<impl Read + Write>,
    contents: &str,
) -> Result<()> {
    // Build code
    let code = QrCode::with_error_correction_level(contents.as_bytes(), EcLevel::L)
        .context("creating QR code")?;
    // qrcode is supposed to be able to generate an Image directly,
    // but that doesn't work.  Take the long way around.
    // https://github.com/kennytm/qrcode-rust/issues/19
    let image_str_with_newlines = code
        .render()
        .module_dimensions(2, 2)
        .dark_color('#')
        .light_color(' ')
        .build();
    let image_str = image_str_with_newlines.replace('\n', "");
    let height = image_str_with_newlines.len() - image_str.len() + 1;
    let width = image_str.len() / height;
    let mut image = RgbImage::new(
        width.try_into().context("invalid QR code width")?,
        height.try_into().context("invalid QR code height")?,
    );
    for (item, pixel) in image_str.chars().zip(image.pixels_mut()) {
        *pixel = if item == '#' {
            Colors::COLOR_BLACK
        } else {
            Colors::COLOR_WHITE
        };
    }

    renderer.write_image(&image)
}

pub(crate) fn write_code128(
    renderer: &mut Renderer<impl Read + Write>,
    contents: &str,
) -> Result<()> {
    // Build code, character set B
    let data = Code128::new(format!("\u{0181}{}", contents))
        .context("creating barcode")?
        .encode();
    // The barcoders image feature pulls in all default features of `image`,
    // which are large.  Handle the conversion ourselves.
    let mut image = RgbImage::new(data.len().try_into().context("barcode size overflow")?, 24);
    for (x, value) in data.iter().enumerate() {
        for y in 0..image.height() {
            *image.get_pixel_mut(x.try_into().context("invalid X coordinate")?, y) = if *value > 0 {
                Colors::COLOR_BLACK
            } else {
                Colors::COLOR_WHITE
            };
        }
    }
    renderer.write_image(&image)
}

pub(crate) struct Colors {
    colors: Vec<<Self as ColorMap>::Color>,
}

impl Colors {
    pub(crate) const COLOR_WHITE: Rgb<u8> = Rgb([255, 255, 255]);
    pub(crate) const COLOR_BLACK: Rgb<u8> = Rgb([0, 0, 0]);
    pub(crate) const COLOR_RED: Rgb<u8> = Rgb([255, 0, 0]);

    fn new(bicolor: bool) -> Self {
        let mut ret = Self {
            colors: vec![Self::COLOR_WHITE, Self::COLOR_BLACK],
        };
        if bicolor {
            ret.colors.push(Self::COLOR_RED);
        }
        ret
    }
}

impl ColorMap for Colors {
    type Color = Rgb<u8>;

    fn index_of(&self, color: &Self::Color) -> usize {
        self.colors.iter().position(|v| v == color).unwrap_or(0)
    }

    fn map_color(&self, color: &mut Self::Color) {
        let mut distance = vec![0i32; self.colors.len()];
        for (i, palette) in self.colors.iter().enumerate() {
            for c in 0..2 {
                let difference = (palette[c] as i32) - (color[c] as i32);
                distance[i] += difference * difference;
            }
        }
        let (i, _) = distance.iter().enumerate().min_by_key(|(_, v)| *v).unwrap();
        *color = self.colors[i];
    }

    fn lookup(&self, index: usize) -> Option<Self::Color> {
        if index < self.colors.len() {
            Some(self.colors[index])
        } else {
            None
        }
    }

    fn has_lookup(&self) -> bool {
        true
    }
}
