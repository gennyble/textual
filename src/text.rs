use std::{borrow::BorrowMut, convert::TryFrom, ops::DerefMut, sync::Arc};

use fontster::{Font, HorizontalAlign, Layout, LayoutSettings};
use small_http::Query;
use smol::lock::{Mutex, RwLock};
use thiserror::Error;

use crate::{
    color::Color,
    image::{ColorProvider, Colors, Image, Mask, Stripes},
    FontProvider,
};

pub struct Text {
    pub text: String,
    pub font: Option<String>,
    pub fontsize: f32,
    pub padding: usize,
    pub color: Color,
    pub bcolor: Color,
    pub pattern: Option<Arc<dyn ColorProvider>>,
    pub align: HorizontalAlign,
    pub forceraw: bool,
    pub aspect: Option<f32>,
    pub outline: bool,
    pub glyph_outline: bool,
    pub baseline: bool,
    pub info: bool,
}

impl Default for Text {
    fn default() -> Self {
        Self {
            text: String::new(),
            font: None,
            fontsize: 128.0,
            padding: 32,
            color: Color::WHITE,
            bcolor: Color::TRANSPARENT,
            pattern: None,
            align: HorizontalAlign::Left,
            forceraw: false,
            aspect: None,
            outline: false,
            glyph_outline: false,
            baseline: false,
            info: false,
        }
    }
}

impl Text {
    //todo: special twitter image. aspect ratio 2:1
    pub async fn make_image(self, fp: &RwLock<FontProvider>) -> Image {
        let font = {
            let mut provider = fp.write().await;
            provider.regular(self.font.clone())
        };

        let settings = LayoutSettings {
            horizontal_align: self.align,
        };

        let mut layout = Layout::new(settings);
        layout.append(font.as_ref(), self.fontsize, &self.text);

        let (horizontal_pad, vertical_pad) = if let Some(ratio) = self.aspect {
            let current_ratio = layout.width() / layout.height();
            println!("{} {}", current_ratio, ratio);
            if ratio > current_ratio {
                // we're too tall! pad the width.
                let needed_padding = (((layout.height() + self.padding as f32) * ratio)
                    - layout.width())
                .ceil() as usize;

                if needed_padding < self.padding {
                    // the added padding is less than the desired. We can't set
                    // the needed to the desired our we'd overshoot
                    (needed_padding, needed_padding - self.padding)
                } else {
                    (needed_padding, self.padding)
                }
            } else if ratio < current_ratio {
                // we're too wide! pad the height
                let needed_padding = (((layout.width() + self.padding as f32) / ratio)
                    - layout.height())
                .ceil() as usize;

                if needed_padding < self.padding {
                    (needed_padding - self.padding, needed_padding)
                } else {
                    (self.padding, needed_padding)
                }
            } else {
                (self.padding, self.padding)
            }
        } else {
            (self.padding, self.padding)
        };
        println!("{} {}", horizontal_pad, vertical_pad);

        let width = layout.width().ceil() as usize + horizontal_pad;
        let height = layout.height().ceil() as usize + vertical_pad;
        let mut image = Image::with_color(width, height, self.bcolor);

        let text_image = if self.pattern.is_some() {
            self.pattern_image(font.as_ref(), layout)
        } else {
            self.normal_image(font.as_ref(), layout)
        };

        image.draw_img(
            text_image,
            horizontal_pad as isize / 2,
            vertical_pad as isize / 2,
        );

        image
    }

    fn pattern_image(&self, font: &Font, layout: Layout) -> Image {
        let width = layout.width().ceil() as usize;
        let height = layout.height().ceil() as usize;

        let mut mask = Mask::new(width, height);
        for glyph in layout.glyphs() {
            let (metrics, raster) = font.rasterize(glyph.c, self.fontsize);

            mask.set_from_buf(
                metrics.width,
                metrics.height,
                &raster,
                glyph.x.ceil() as isize,
                glyph.y.ceil() as isize,
            )
        }

        let mut pattern = Image::from_provider(width, height, self.pattern.as_deref().unwrap());
        pattern.mask(mask, 0, 0);

        pattern
    }

    fn normal_image(&self, font: &Font, layout: Layout) -> Image {
        let width = layout.width().ceil() as usize;
        let height = layout.height().ceil() as usize;
        let mut image = Image::with_color(width, height, Color::TRANSPARENT);

        for glyph in layout.glyphs() {
            let (metrics, raster) = font.rasterize(glyph.c, self.fontsize);
            let glyph_img = Image::from_buffer(
                metrics.width,
                metrics.height,
                raster,
                Colors::GreyAsAlpha(self.color),
            );

            image.draw_img(glyph_img, glyph.x.ceil() as isize, glyph.y.ceil() as isize)
        }

        image
    }

    fn color<S: AsRef<str>>(s: S) -> Option<Color> {
        match s.as_ref() {
            "transparent" => return Some(Color::TRANSPARENT),
            "black" => return Some(Color::BLACK),
            "red" => return Some(Color::RED),
            "green" => return Some(Color::GREEN),
            "blue" => return Some(Color::BLUE),
            "yellow" => return Some(Color::YELLOW),
            "fuchsia" | "magenta" => return Some(Color::FUCHSIA),
            "aqua" | "cyan" => return Some(Color::AQUA),
            "white" => return Some(Color::WHITE),
            _ => (),
        }

        let hexpair = |p: &[char]| -> Option<u8> {
            if let (Some(u), Some(l)) = (p[0].to_digit(16), p[1].to_digit(16)) {
                return Some((u * 16 + l) as u8);
            }
            None
        };

        // Maybe it's a full RGB hex?
        if s.as_ref().len() == 6 {
            let chars: Vec<char> = s.as_ref().chars().collect();
            let mut components: Vec<u8> = chars.chunks(2).filter_map(hexpair).collect();
            components.push(255);

            if let Ok(clr) = Color::try_from(&components[..]) {
                return Some(clr);
            }
        }

        // Full RGBA hex?
        if s.as_ref().len() == 8 {
            let chars: Vec<char> = s.as_ref().chars().collect();
            let components: Vec<u8> = chars.chunks(2).filter_map(hexpair).collect();

            if let Ok(clr) = Color::try_from(&components[..]) {
                return Some(clr);
            }
        }

        // Half RGB hex?
        if s.as_ref().len() == 3 {
            let mut components: Vec<u8> = s
                .as_ref()
                .chars()
                .filter_map(|c| hexpair(&[c, c]))
                .collect();
            components.push(255);

            if let Ok(clr) = Color::try_from(&components[..]) {
                return Some(clr);
            }
        }

        // Half RGBA hex?
        if s.as_ref().len() == 4 {
            let components: Vec<u8> = s
                .as_ref()
                .chars()
                .filter_map(|c| hexpair(&[c, c]))
                .collect();

            if let Ok(clr) = Color::try_from(&components[..]) {
                return Some(clr);
            }
        }

        None
    }

    fn color_or<S: AsRef<str>>(string: Option<S>, color: Color) -> Color {
        if let Some(string) = string {
            Self::color(string).unwrap_or(color)
        } else {
            color
        }
    }
}

impl TryFrom<Query> for Text {
    type Error = TextError;

    fn try_from(query: Query) -> Result<Self, Self::Error> {
        let text = match query.get_first_value("text") {
            Some(t) => {
                if t.is_empty() {
                    return Err(TextError::NoText);
                } else {
                    t.into()
                }
            }
            None => return Err(TextError::NoText),
        };

        let longshort = |long, short| query.get_first_value(long).or(query.get_first_value(short));

        let fontsize = longshort("fontsize", "fs")
            .map(|s| s.parse::<f32>().unwrap_or(128.0))
            .unwrap_or(128.0);

        let padding = query
            .get_first_value("pad")
            .map(|s| s.parse::<f32>().unwrap_or(-0.25))
            .map(|f| if f < 0.0 { fontsize * -f } else { f })
            .unwrap_or(fontsize * 0.25) as usize;

        let align = match query.get_first_value("align").as_deref() {
            Some("center") => HorizontalAlign::Center,
            Some("right") => HorizontalAlign::Right,
            _ => HorizontalAlign::Left,
        };

        let pattern: Option<Arc<dyn ColorProvider>> =
            match query.get_first_value("pattern").as_deref() {
                Some("trans") => Some(Arc::new(Stripes {
                    colors: vec![(85, 205, 252).into(), Color::WHITE, (247, 168, 184).into()],
                    stripe_width: fontsize as usize / 8,
                    slope: 2.0,
                })),
                Some("nonbinary") => Some(Arc::new(Stripes {
                    colors: vec![
                        (255, 244, 48).into(),
                        Color::WHITE,
                        (156, 89, 209).into(),
                        Color::BLACK,
                    ],
                    stripe_width: fontsize as usize / 8,
                    slope: 2.0,
                })),
                _ => None,
            };

        let aspect = query
            .get_first_value("aspect")
            .map(|s| s.parse::<f32>().ok())
            .flatten();

        Ok(Self {
            text,
            align,
            font: query.get_first_value("font").map(|s| s.into()),
            fontsize,
            padding,
            color: Self::color_or(longshort("color", "c"), Color::WHITE),
            bcolor: Self::color_or(longshort("bcolor", "bc"), Color::TRANSPARENT),
            pattern,
            forceraw: query.has_bool("forceraw"),
            aspect,
            outline: query.has_bool("outline"),
            glyph_outline: query.has_bool("glyph_outline"),
            baseline: query.has_bool("baseline"),
            info: query.has_bool("info"),
        })
    }
}

#[derive(Error, Debug)]
pub enum TextError {
    #[error("Text to rasterize must be provided")]
    NoText,
}
