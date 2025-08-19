use std::{fmt, str::FromStr};

use serde::{de, Deserialize, Deserializer};

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct FontVariant {
	pub weight: FontWeight,
	pub style: FontStyle,
}

impl FontVariant {
	pub fn new(weight: FontWeight, style: FontStyle) -> Self {
		FontVariant { weight, style }
	}

	pub fn with_weight(weight: FontWeight) -> Self {
		FontVariant {
			weight,
			..Default::default()
		}
	}

	pub fn with_style(style: FontStyle) -> Self {
		FontVariant {
			style,
			..Default::default()
		}
	}
}

impl fmt::Display for FontVariant {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{} {}", self.weight, self.style)
	}
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum FontStyle {
	Normal,
	Italic,
	Oblique,
}

impl Default for FontStyle {
	fn default() -> Self {
		FontStyle::Normal
	}
}

impl FromStr for FontStyle {
	type Err = FontVariantParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"normal" => Ok(FontStyle::Normal),
			"italic" => Ok(FontStyle::Italic),
			"oblique" => Ok(FontStyle::Oblique),
			_ => Err(FontVariantParseError::UnknownStyleName {
				style: s.to_owned(),
			}),
		}
	}
}

impl<'de> Deserialize<'de> for FontStyle {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		FromStr::from_str(&s).map_err(de::Error::custom)
	}
}

impl fmt::Display for FontStyle {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let name = match self {
			FontStyle::Normal => "normal",
			FontStyle::Italic => "italic",
			FontStyle::Oblique => "oblique",
		};

		write!(f, "{}", name)
	}
}

/// Font weight names. List taken from here: https://en.wikipedia.org/wiki/Font#Weight
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum FontWeight {
	Thin,
	ExtraLight,
	Light,
	Regular,
	Medium,
	SemiBold,
	Bold,
	ExtraBold,
	Black,
	ExtraBlack,
}

impl FontWeight {
	// https://developer.mozilla.org/en-US/docs/Web/CSS/font-weight#common_weight_name_mapping
	pub fn into_weight_number(&self) -> usize {
		match self {
			FontWeight::Thin => 100,
			FontWeight::ExtraLight => 200,
			FontWeight::Light => 300,
			FontWeight::Regular => 400,
			FontWeight::Medium => 500,
			FontWeight::SemiBold => 600,
			FontWeight::Bold => 700,
			FontWeight::ExtraBold => 800,
			FontWeight::Black => 900,
			FontWeight::ExtraBlack => 950,
		}
	}
}

impl Default for FontWeight {
	fn default() -> Self {
		FontWeight::Regular
	}
}

impl FromStr for FontWeight {
	type Err = FontVariantParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		// gen- the urge to panic on "book" is high, but BUT! I will not.
		match s.to_lowercase().as_str() {
			"thin" | "100" => Ok(FontWeight::Thin),
			"extralight" | "extra-light" | "ultralight" | "ultra-light" | "200" => {
				Ok(FontWeight::ExtraLight)
			}
			"light" | "300" => Ok(FontWeight::Light),
			"normal" | "regular" | "400" => Ok(FontWeight::Regular),
			"medium" | "500" => Ok(FontWeight::Medium),
			"semibold" | "semi-bold" | "demibold" | "demi-bold" | "600" => Ok(FontWeight::SemiBold),
			"bold" | "700" => Ok(FontWeight::Bold),
			"extrabold" | "extra-bold" | "ultrabold" | "ultra-bold" | "800" => {
				Ok(FontWeight::ExtraBold)
			}
			"black" | "heavy" | "900" => Ok(FontWeight::Black),
			"extrablack" | "extra-black" | "ultrablack" | "ultra-black" | "950" => {
				Ok(FontWeight::ExtraBlack)
			}
			_ => Err(FontVariantParseError::UnknownWeightName {
				weight: s.to_owned(),
			}),
		}
	}
}

impl<'de> Deserialize<'de> for FontWeight {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		FromStr::from_str(&s).map_err(de::Error::custom)
	}
}

impl fmt::Display for FontWeight {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let name = match self {
			FontWeight::Thin => "thin",
			FontWeight::ExtraLight => "extralight",
			FontWeight::Light => "light",
			FontWeight::Regular => "regular",
			FontWeight::Medium => "medium",
			FontWeight::SemiBold => "semibold",
			FontWeight::Bold => "bold",
			FontWeight::ExtraBold => "extrabold",
			FontWeight::Black => "black",
			FontWeight::ExtraBlack => "extrablack",
		};

		write!(f, "{}", name)
	}
}

#[derive(Debug, thiserror::Error)]
pub enum FontVariantParseError {
	#[error("The style {style} is not recognised")]
	UnknownStyleName { style: String },
	#[error("The weight {weight} is not recognised")]
	UnknownWeightName { weight: String },
}
