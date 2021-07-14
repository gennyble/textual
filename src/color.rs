use std::convert::TryFrom;

#[derive(Debug, PartialEq, Copy, Clone, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Color = Color::new(0, 0, 0, 0);
    pub const BLACK: Color = Color::new(0, 0, 0, 255);
    pub const RED: Color = Color::new(255, 0, 0, 255);
    pub const GREEN: Color = Color::new(0, 255, 0, 255);
    pub const BLUE: Color = Color::new(0, 0, 255, 255);

    pub const YELLOW: Color = Color::new(255, 255, 0, 255);
    pub const FUCHSIA: Color = Color::new(255, 0, 255, 255);
    pub const AQUA: Color = Color::new(0, 255, 255, 255);

    pub const WHITE: Color = Color::new(255, 255, 255, 255);

    pub const fn new(r: u8, b: u8, g: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

impl From<Color> for [u8; 4] {
    fn from(col: Color) -> Self {
        [col.r, col.g, col.b, col.a]
    }
}

impl From<(u8, u8, u8)> for Color {
    fn from(rgb: (u8, u8, u8)) -> Self {
        Self {
            r: rgb.0,
            g: rgb.1,
            b: rgb.2,
            a: 255,
        }
    }
}

impl From<(u8, u8, u8, u8)> for Color {
    fn from(rgba: (u8, u8, u8, u8)) -> Self {
        Self {
            r: rgba.0,
            g: rgba.1,
            b: rgba.2,
            a: rgba.3,
        }
    }
}

impl TryFrom<&[u8]> for Color {
    type Error = ();

    fn try_from(rgba: &[u8]) -> Result<Self, Self::Error> {
        if rgba.len() == 4 {
            Ok(Self {
                r: rgba[0],
                g: rgba[1],
                b: rgba[2],
                a: rgba[3],
            })
        } else {
            Err(())
        }
    }
}
