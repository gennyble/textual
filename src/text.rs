use std::{convert::TryFrom, sync::Arc};

use fontster::{HorizontalAlign, Layout, LayoutSettings};
use smol::lock::Mutex;
use thiserror::Error;

use crate::{
    color::Color,
    image::{ColorProvider, Colors, Image, Stripes},
    query::Query,
    FontProvider,
};

pub struct Text {
    pub text: String,
    pub font: Option<String>,
    pub fontsize: f32,
    pub padding: usize,
    pub color: Color,
    bcolor: Color,
    pattern: Option<Arc<dyn ColorProvider>>,
    align: HorizontalAlign,
    pub forceraw: bool,
    pub outline: bool,
    pub glyph_outline: bool,
    pub baseline: bool,
}

impl Text {
    //todo: special twitter image. aspect ratio 2:1
    pub async fn make_image(self, fp: &Mutex<FontProvider>) -> Image {
        let font = fp.lock().await.regular(self.font.clone());

        let settings = LayoutSettings {
            horizontal_align: self.align,
        };

        let mut layout = Layout::new(settings);
        layout.append(font.as_ref(), self.fontsize, &self.text);

        let width = layout.width().ceil() as usize + self.padding;
        let height = layout.height().ceil() as usize + self.padding;
        let mut image = Image::with_color(width, height, self.bcolor);

        for glyph in layout.glyphs() {
            let (metrics, raster) = font.rasterize(glyph.c, self.fontsize);
            let glyph_img = if let Some(pat) = &self.pattern {
                Image::from_buffer(
                    metrics.width,
                    metrics.height,
                    raster,
                    Colors::GreyAsMask(pat.as_ref()),
                )
            } else {
                Image::from_buffer(
                    metrics.width,
                    metrics.height,
                    raster,
                    Colors::GreyAsAlpha(self.color),
                )
            };

            image.draw_img(
                glyph_img,
                glyph.x.ceil() as isize + self.padding as isize / 2,
                glyph.y.ceil() as isize + self.padding as isize / 2,
            )
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
                    t
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

        Ok(Self {
            text,
            align,
            font: query.get_first_value("font"),
            fontsize,
            padding,
            color: Self::color_or(longshort("color", "c"), Color::WHITE),
            bcolor: Self::color_or(longshort("bcolor", "bc"), Color::TRANSPARENT),
            pattern,
            forceraw: query.bool_present("forceraw"),
            outline: query.bool_present("outline"),
            glyph_outline: query.bool_present("glyph_outline"),
            baseline: query.bool_present("baseline"),
        })
    }
}

#[derive(Error, Debug)]
pub enum TextError {
    #[error("Text to rasterize must be provided")]
    NoText,
}
