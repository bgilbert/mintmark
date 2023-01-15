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
use base64::Engine;
use qrcode::{EcLevel, QrCode};
use std::borrow::Cow;
use std::io::{Read, Write};

use crate::render::Renderer;
use crate::strike::{Strike, StrikeColors, StrikeImage};
use crate::FormatInfo;

pub(crate) fn write_bitmap(
    renderer: &mut Renderer<impl Read + Write>,
    contents: &str,
) -> Result<()> {
    let width = contents.split('\n').fold(0, |acc, l| acc.max(l.len()));
    let height = contents.split('\n').count();
    let mut image = StrikeImage::from_pixel(
        width.try_into().context("invalid bitmap width")?,
        height.try_into().context("invalid bitmap height")?,
        Strike([0, 0]),
    );
    for (y, row) in contents.split('\n').enumerate() {
        for (x, value) in row.chars().enumerate() {
            *image.get_pixel_mut(
                x.try_into().context("invalid X coordinate")?,
                y.try_into().context("invalid Y coordinate")?,
            ) = if value != ' ' {
                Strike([1, 0])
            } else {
                Strike([0, 0])
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
        Cow::from(
            base64::engine::general_purpose::STANDARD
                .decode(contents.replace(['\r', '\n'], ""))
                .context("decoding base64")?,
        )
    } else {
        Cow::from(contents.as_bytes())
    };
    let image = image::load_from_memory(&data)?.to_rgb8();
    renderer.write_image(&StrikeColors::new(bicolor).map_image(&image))
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
    let mut image = StrikeImage::new(
        width.try_into().context("invalid QR code width")?,
        height.try_into().context("invalid QR code height")?,
    );
    for (item, pixel) in image_str.chars().zip(image.pixels_mut()) {
        *pixel = if item == '#' {
            Strike([1, 0])
        } else {
            Strike([0, 0])
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
    let mut image = StrikeImage::new(data.len().try_into().context("barcode size overflow")?, 24);
    for (x, value) in data.iter().enumerate() {
        for y in 0..image.height() {
            *image.get_pixel_mut(x.try_into().context("invalid X coordinate")?, y) = if *value > 0 {
                Strike([1, 0])
            } else {
                Strike([0, 0])
            };
        }
    }
    renderer.write_image(&image)
}
