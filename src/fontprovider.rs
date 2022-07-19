use core::fmt;
use std::{
	io::{self, Read, Write},
	path::{Path, PathBuf},
	str::FromStr,
	sync::Arc,
	time::Instant,
};

use fontster::Font;
use serde_json::Value;
use std::fs::File;
use thiserror::Error;

struct FontCache {
	location: PathBuf,
	fonts: Vec<FontFamily>,
}

impl FontCache {
	fn new<P: Into<PathBuf>>(location: P) -> io::Result<Self> {
		let mut cache = FontCache {
			location: location.into(),
			fonts: vec![],
		};

		cache.populate().unwrap();

		Ok(cache)
	}

	fn family<S: AsRef<str>>(&self, name: S) -> Option<&FontFamily> {
		for font in &self.fonts {
			if font.face == name.as_ref() {
				return Some(font);
			}
		}

		None
	}

	fn family_mut<S: AsRef<str>>(&mut self, name: S) -> Option<&mut FontFamily> {
		for font in self.fonts.iter_mut() {
			if font.face == name.as_ref() {
				return Some(font);
			}
		}

		None
	}

	fn regular<S: AsRef<str>>(&self, fam: S) -> Option<Font> {
		self.variant(fam, FontVariant::default())
	}

	pub fn variant<F: AsRef<str>>(&self, family: F, variant: FontVariant) -> Option<Font> {
		if let Some(fam) = self.family(family.as_ref()) {
			if let Some(path) = fam.variant_path(variant) {
				let mut file = File::open(path).unwrap();

				let mut buffer = vec![];
				file.read_to_end(&mut buffer).unwrap();

				return Some(fontster::parse_font(&mut buffer).unwrap());
			}
		}

		None
	}

	fn populate(&mut self) -> io::Result<()> {
		let dir = std::fs::read_dir(&self.location)?;

		for entry in dir {
			let entry = entry.unwrap();
			let path = entry.path();
			let fname = path.file_stem().unwrap().to_str().unwrap();

			let (family, variant) = match fname.rsplit_once('-') {
				Some((family, variant)) => match variant.split_once(' ') {
					Some((weight, style)) => {
						let style = match style.parse() {
							Ok(style) => style,
							Err(e) => {
								eprintln!("Unable to recognise font style for {}", fname);
								continue;
							}
						};

						let weight = match weight.parse() {
							Ok(weight) => weight,
							Err(e) => {
								eprintln!("Unable to recognise font weight for {}", fname);
								continue;
							}
						};

						(family, FontVariant::new(weight, style))
					}
					None => {
						eprintln!("Unable to recognise variant for {}", fname);
						continue;
					}
				},
				_ => {
					eprintln!("Unknown file in cache: {}", fname);
					continue;
				}
			};

			let ftype = entry.file_type().unwrap();

			if ftype.is_file() {
				if let Some(fam) = self.family_mut(family) {
					fam.push(variant, entry.path().to_str().unwrap());
				} else {
					let mut fam = FontFamily::new(family);
					fam.push(variant, entry.path().to_str().unwrap());

					self.fonts.push(fam);
				}
			}
		}

		println!("{} files in cache", self.fonts.len());

		Ok(())
	}

	fn save_font<F: AsRef<str>>(&mut self, family: F, variant: FontVariant, buf: &[u8]) {
		let family = family.as_ref();

		let fname = format!("{}-{} {}.ttf", family, variant.weight, variant.style);
		let mut path = self.location.clone();
		path.push(fname);

		let mut file = File::create(&path).unwrap();
		file.write_all(buf).unwrap();

		if let Some(family) = self.family_mut(family) {
			family.push(variant, path.to_string_lossy())
		} else {
			let mut fam = FontFamily::new(family);
			fam.push(variant, path.to_string_lossy());

			self.fonts.push(fam);
		}

		println!("saved font {}", path.to_str().unwrap());
	}
}

pub struct FontProvider {
	default: Arc<Font>,
	fonts: Vec<FontFamily>,
	font_cache: FontCache,
}

impl FontProvider {
	pub fn new<P: AsRef<Path>>(fontcache: P, google_fonts_apikey: &str) -> Self {
		let google = get_fonts_from_google(google_fonts_apikey).unwrap();

		Self {
			default: Arc::new(
				fontster::parse_font(include_bytes!("../Cabin-Regular.ttf")).unwrap(),
			),
			fonts: google,
			font_cache: FontCache::new(fontcache.as_ref()).unwrap(),
		}
	}

	pub fn cached(&self) -> usize {
		self.font_cache
			.fonts
			.iter()
			.fold(0, |acc, fam| acc + fam.variants.len())
	}

	fn push(&mut self, fam: FontFamily) {
		self.fonts.push(fam);
	}

	fn family<S: AsRef<str>>(&self, face: S) -> Option<&FontFamily> {
		for font in &self.fonts {
			if font.face == face.as_ref() {
				return Some(font);
			}
		}

		None
	}

	pub fn variant<F: Into<String>>(&mut self, family: F, variant: FontVariant) -> Arc<Font> {
		let family_string = family.into();

		if let Some(font) = self.font_cache.variant(&family_string, variant) {
			println!("hit cache for {} {}", family_string, variant);

			return Arc::new(font);
		} else if let Some(family) = self.family(&family_string) {
			println!("missed cache for {} {}", family_string, variant);

			if let Some(var) = family.variant_path(variant).map(<_>::to_owned) {
				let response = ureq::get(&var).call().unwrap();

				let mut buffer: Vec<u8> = Vec::new();
				response.into_reader().read_to_end(&mut buffer).unwrap();

				self.font_cache.save_font(family_string, variant, &buffer);

				return Arc::new(fontster::parse_font(&buffer).unwrap());
			}
		}

		self.default.clone()
	}

	pub fn regular<S: AsRef<str>>(&mut self, fam: S) -> Arc<Font> {
		self.variant(fam.as_ref(), FontVariant::default())
	}

	pub fn default_font(&self) -> Arc<Font> {
		self.default.clone()
	}
}

fn get_fonts_from_google<S: AsRef<str>>(apikey: S) -> Result<Vec<FontFamily>, ureq::Error> {
	let api_str = format!(
		"https://www.googleapis.com/webfonts/v1/webfonts?key={}",
		apikey.as_ref()
	);

	let before = Instant::now();
	let response = ureq::get(&api_str).call()?;
	let json: Value = serde_json::from_str(&response.into_string()?).unwrap();

	let fonts = match &json["items"] {
		Value::Array(fonts) => fonts,
		_ => panic!(),
	};

	let mut ret = vec![];

	for item in fonts {
		let name = item["family"].as_str().unwrap();
		let mut family = FontFamily::new(name);

		for (style, filepath) in item["files"].as_object().unwrap() {
			// Font styles can be one of three things...
			let variant = if style == "regular" {
				// ...just the word "regular" which means normal weight and style
				FontVariant::default()
			} else if let Some(weight) = style.strip_suffix("italic") {
				// ...###italic where ### is a weight, like 400
				FontVariant::new(weight.parse().unwrap_or_default(), FontStyle::Italic)
			} else {
				// ...just the weight
				FontVariant::with_weight(style.parse().unwrap())
			};

			family.push(variant, filepath.as_str().unwrap());
		}

		ret.push(family);
	}

	println!(
		"getting font list took {}s",
		Instant::now().duration_since(before).as_secs()
	);

	Ok(ret)
}

struct FontFamily {
	face: String,
	variants: Vec<(FontVariant, String)>,
}

impl FontFamily {
	fn new<S: Into<String>>(face: S) -> Self {
		FontFamily {
			face: face.into(),
			variants: vec![],
		}
	}

	fn push<P: Into<String>>(&mut self, variant: FontVariant, path: P) {
		self.variants.push((variant, path.into()));
	}

	/// Could be a filepath or a URL depending on how you're using this.
	/// FontProvider stores URLs, FontCache local files
	fn variant_path(&self, variant: FontVariant) -> Option<&String> {
		for (our_varient, path) in &self.variants {
			if *our_varient == variant {
				return Some(path);
			}
		}

		None
	}
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct FontVariant {
	weight: FontWeight,
	style: FontStyle,
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

#[derive(Debug, Error)]
pub enum FontVariantParseError {
	#[error("The style {style} is not recognised")]
	UnknownStyleName { style: String },
	#[error("The weight {weight} is not recognised")]
	UnknownWeightName { weight: String },
}
