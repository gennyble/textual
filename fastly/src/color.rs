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

	pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
		Self { r, g, b, a }
	}

	pub fn as_hex(&self) -> String {
		format!("{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
	}

	/// This is *not* how colour is mixed, but that's okay.
	///
	/// only scales RGB, leaving the alpha channel alone
	pub fn scale_rgb(&self, scalar: f32) -> Color {
		Self {
			r: (self.r as f32 * scalar).clamp(0.0, 255.0) as u8,
			g: (self.g as f32 * scalar).clamp(0.0, 255.0) as u8,
			b: (self.b as f32 * scalar).clamp(0.0, 255.0) as u8,
			a: self.a,
		}
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

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn as_hex() {
		assert_eq!(Color::WHITE.as_hex(), "FFFFFFFF");
		assert_eq!(Color::BLACK.as_hex(), "000000FF");

		assert_eq!(Color::RED.as_hex(), "FF0000FF");
		assert_eq!(Color::GREEN.as_hex(), "00FF00FF");
		assert_eq!(Color::BLUE.as_hex(), "0000FFFF");
		assert_eq!(Color::TRANSPARENT.as_hex(), "00000000");
	}
}
