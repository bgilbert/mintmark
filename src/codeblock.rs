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
use std::rc::Rc;

use crate::render::{Format, FormatFlags, Renderer};
use crate::strike::{Strike, StrikeColors, StrikeImage};

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum CodeBlockConfig {
    Bitmap(BitmapBlock),
    Code128(Code128Block),
    Image(ImageBlock),
    QrCode(QrCodeBlock),
    Text(TextBlock),
}

impl CodeBlockConfig {
    pub(crate) fn from_info(info: &str) -> Result<Self> {
        let mut it = info.split_whitespace();
        let language = it.next().unwrap_or("");
        let options = it.collect::<Vec<&str>>();
        use CodeBlockConfig::*;
        Ok(match language {
            "bitmap" => Bitmap(BitmapBlock::from_options(&options)?),
            "code128" => Code128(Code128Block::from_options(&options)?),
            "image" => Image(ImageBlock::from_options(&options)?),
            "qrcode" => QrCode(QrCodeBlock::from_options(&options)?),
            "text" => Text(TextBlock::from_options(&options)?),
            _ => Text(TextBlock::default()),
        })
    }

    pub(crate) fn render(
        &self,
        renderer: &mut Renderer<impl Read + Write>,
        contents: &str,
    ) -> Result<()> {
        use CodeBlockConfig::*;
        match self {
            Bitmap(block) => block.render(renderer, contents),
            Code128(block) => block.render(renderer, contents),
            Image(block) => block.render(renderer, contents),
            QrCode(block) => block.render(renderer, contents),
            Text(block) => block.render(renderer, contents),
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct BitmapBlock {
    bold: bool,
}

impl BitmapBlock {
    fn from_options(options: &[&str]) -> Result<Self> {
        let mut block = Self::default();
        for option in options {
            match *option {
                "bold" => block.bold = true,
                _ => bail!("unknown option '{}'", option),
            }
        }
        Ok(block)
    }

    fn render(&self, renderer: &mut Renderer<impl Read + Write>, contents: &str) -> Result<()> {
        let contents = contents.trim_end_matches('\n');
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
                    if self.bold {
                        Strike([2, 0])
                    } else {
                        Strike([1, 0])
                    }
                } else {
                    Strike([0, 0])
                };
            }
        }
        renderer.write_image(&image)
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct Code128Block {
    bold: bool,
}

impl Code128Block {
    fn from_options(options: &[&str]) -> Result<Self> {
        let mut block = Self::default();
        for option in options {
            match *option {
                "bold" => block.bold = true,
                _ => bail!("unknown option '{}'", option),
            }
        }
        Ok(block)
    }

    fn render(&self, renderer: &mut Renderer<impl Read + Write>, contents: &str) -> Result<()> {
        // Build code, character set B
        let data = Code128::new(format!("\u{0181}{}", contents.trim()))
            .context("creating barcode")?
            .encode();
        // The barcoders image feature pulls in all default features of `image`,
        // which are large.  Handle the conversion ourselves.
        let mut image =
            StrikeImage::new(data.len().try_into().context("barcode size overflow")?, 24);
        for (x, value) in data.iter().enumerate() {
            for y in 0..image.height() {
                *image.get_pixel_mut(x.try_into().context("invalid X coordinate")?, y) =
                    if *value > 0 {
                        if self.bold {
                            Strike([2, 0])
                        } else {
                            Strike([1, 0])
                        }
                    } else {
                        Strike([0, 0])
                    };
            }
        }
        renderer.write_image(&image)
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct ImageBlock {
    base64: bool,
    bicolor: bool,
}

impl ImageBlock {
    fn from_options(options: &[&str]) -> Result<Self> {
        let mut block = ImageBlock::default();
        for option in options {
            match *option {
                "base64" => block.base64 = true,
                "bicolor" => block.bicolor = true,
                _ => bail!("unknown option '{}'", option),
            }
        }
        Ok(block)
    }

    fn render(&self, renderer: &mut Renderer<impl Read + Write>, contents: &str) -> Result<()> {
        let data = base64_maybe_decode(contents, self.base64)?;
        let image = image::load_from_memory(&data)?.into_rgb8();
        renderer.write_image(&StrikeColors::new(self.bicolor).map_image(&image))
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct QrCodeBlock {
    base64: bool,
    bold: bool,
}

impl QrCodeBlock {
    fn from_options(options: &[&str]) -> Result<Self> {
        let mut block = Self::default();
        for option in options {
            match *option {
                "base64" => block.base64 = true,
                "bold" => block.bold = true,
                _ => bail!("unknown option '{}'", option),
            }
        }
        Ok(block)
    }

    fn render(&self, renderer: &mut Renderer<impl Read + Write>, contents: &str) -> Result<()> {
        // Build code
        let data = base64_maybe_decode(contents.trim(), self.base64)?;
        let code =
            QrCode::with_error_correction_level(data, EcLevel::L).context("creating QR code")?;
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
                if self.bold {
                    Strike([2, 0])
                } else {
                    Strike([1, 0])
                }
            } else {
                Strike([0, 0])
            };
        }

        renderer.write_image(&image)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct TextBlock {
    format: Rc<Format>,
}

impl Default for TextBlock {
    fn default() -> Self {
        Self {
            format: Format::new().with_red(true),
        }
    }
}

impl TextBlock {
    fn from_options(options: &[&str]) -> Result<Self> {
        let mut block = Self::default();
        for option in options {
            block.format = match *option {
                "black" => block.format.with_red(false),
                "bold" => block.format.with_flags(FormatFlags::EMPHASIZED),
                "doubleheight" => block.format.with_flags(FormatFlags::DOUBLE_HEIGHT),
                "doublewidth" => block.format.with_flags(FormatFlags::DOUBLE_WIDTH),
                "strikethrough" => block.format.with_strikethrough(true),
                "underline" => block.format.with_flags(FormatFlags::UNDERLINE),
                "wide" => block.format.without_flags(FormatFlags::NARROW),
                _ => bail!("unknown option '{}'", option),
            }
        }
        Ok(block)
    }

    fn render(&self, renderer: &mut Renderer<impl Read + Write>, contents: &str) -> Result<()> {
        renderer.set_format(self.format.clone());
        let result = renderer.write(contents);
        renderer.restore_format();
        result
    }
}

fn base64_maybe_decode(contents: &str, base64: bool) -> Result<Cow<[u8]>> {
    if base64 {
        Ok(Cow::from(
            base64::engine::general_purpose::STANDARD
                .decode(contents.replace(['\r', '\n'], ""))
                .context("decoding base64")?,
        ))
    } else {
        Ok(Cow::from(contents.as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_block_parse_success() {
        let tests = [
            ("", CodeBlockConfig::Text(TextBlock::default())),
            ("foo", CodeBlockConfig::Text(TextBlock::default())),
            ("  text	", CodeBlockConfig::Text(TextBlock::default())),
            (
                "text black",
                CodeBlockConfig::Text(TextBlock {
                    format: Format::new(),
                }),
            ),
            (
                " text  black  bold ",
                CodeBlockConfig::Text(TextBlock {
                    format: Format::new().with_flags(FormatFlags::EMPHASIZED),
                }),
            ),
        ];
        for (info, expected) in tests {
            assert_eq!(CodeBlockConfig::from_info(info).unwrap(), expected);
        }
    }

    #[test]
    fn code_block_parse_error() {
        let tests = [
            "text bold blah",
            "image foo",
            "bitmap foo",
            "code128 foo",
            "qrcode foo",
        ];
        for info in tests {
            CodeBlockConfig::from_info(info).unwrap_err();
        }
    }
}
