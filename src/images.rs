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
use image::{ImageBuffer, Luma, LumaA, Pixel, Rgb, RgbImage, Rgba};
use qrcode::{EcLevel, QrCode};
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::iter::zip;

use crate::render::Renderer;
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
        Cow::from(base64::decode(contents.replace(['\r', '\n'], "")).context("decoding base64")?)
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

struct StrikeColors {
    colors: Vec<<Self as ColorMap>::Color>,
    map: HashMap<<Self as ColorMap>::Color, Strike>,
}

impl StrikeColors {
    fn new(bicolor: bool) -> Self {
        let mut map = HashMap::from([
            (Rgb([255, 255, 255]), Strike([0, 0])),
            (Rgb([0, 0, 0]), Strike([1, 0])),
        ]);
        if bicolor {
            map.insert(Rgb([255, 0, 0]), Strike([0, 1]));
        }
        Self {
            colors: map.keys().cloned().collect(),
            map,
        }
    }

    fn map_image(&self, image: &RgbImage) -> StrikeImage {
        let mut dithered = image.clone();
        dither(&mut dithered, self);
        let mut ret = StrikeImage::new(image.width(), image.height());
        for (orig, mapped) in zip(dithered.pixels(), ret.pixels_mut()) {
            *mapped = *self.map.get(orig).expect("unexpected pixel value");
        }
        ret
    }
}

impl ColorMap for StrikeColors {
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

/// The number of strikes that should be used for each of the black and red
/// channels, respectively.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Strike(pub [u8; 2]);

impl Pixel for Strike {
    type Subpixel = u8;
    const CHANNEL_COUNT: u8 = 2;
    const COLOR_MODEL: &'static str = "BlR";

    fn channels(&self) -> &[Self::Subpixel] {
        &self.0
    }

    fn channels_mut(&mut self) -> &mut [Self::Subpixel] {
        &mut self.0
    }

    fn channels4(
        &self,
    ) -> (
        Self::Subpixel,
        Self::Subpixel,
        Self::Subpixel,
        Self::Subpixel,
    ) {
        (self.0[0], self.0[1], 0, 0)
    }

    fn from_channels(
        a: Self::Subpixel,
        b: Self::Subpixel,
        _: Self::Subpixel,
        _: Self::Subpixel,
    ) -> Self {
        Self([a, b])
    }

    fn from_slice(slice: &[Self::Subpixel]) -> &Self {
        // copied from image color.rs
        assert_eq!(slice.len(), Self::CHANNEL_COUNT as usize);
        unsafe { &*(slice.as_ptr() as *const Self) }
    }

    fn from_slice_mut(slice: &mut [Self::Subpixel]) -> &mut Self {
        // copied from image color.rs
        assert_eq!(slice.len(), Self::CHANNEL_COUNT as usize);
        unsafe { &mut *(slice.as_mut_ptr() as *mut Self) }
    }

    fn to_rgb(&self) -> Rgb<Self::Subpixel> {
        unimplemented!()
    }

    fn to_rgba(&self) -> Rgba<Self::Subpixel> {
        unimplemented!()
    }

    fn to_luma(&self) -> Luma<Self::Subpixel> {
        unimplemented!()
    }

    fn to_luma_alpha(&self) -> LumaA<Self::Subpixel> {
        unimplemented!()
    }

    fn map<F>(&self, _: F) -> Self
    where
        F: FnMut(Self::Subpixel) -> Self::Subpixel,
    {
        unimplemented!()
    }

    fn apply<F>(&mut self, _: F)
    where
        F: FnMut(Self::Subpixel) -> Self::Subpixel,
    {
        unimplemented!()
    }

    fn map_with_alpha<F, G>(&self, _: F, _: G) -> Self
    where
        F: FnMut(Self::Subpixel) -> Self::Subpixel,
        G: FnMut(Self::Subpixel) -> Self::Subpixel,
    {
        unimplemented!()
    }

    fn apply_with_alpha<F, G>(&mut self, _: F, _: G)
    where
        F: FnMut(Self::Subpixel) -> Self::Subpixel,
        G: FnMut(Self::Subpixel) -> Self::Subpixel,
    {
        unimplemented!()
    }

    fn map2<F>(&self, _: &Self, _: F) -> Self
    where
        F: FnMut(Self::Subpixel, Self::Subpixel) -> Self::Subpixel,
    {
        unimplemented!()
    }

    fn apply2<F>(&mut self, _: &Self, _: F)
    where
        F: FnMut(Self::Subpixel, Self::Subpixel) -> Self::Subpixel,
    {
        unimplemented!()
    }

    fn invert(&mut self) {
        unimplemented!()
    }

    fn blend(&mut self, _: &Self) {
        unimplemented!()
    }
}

pub type StrikeImage = ImageBuffer<Strike, Vec<u8>>;
