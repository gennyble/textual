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
	fonts: Vec<Family>,
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

	fn family<S: AsRef<str>>(&self, name: S) -> Option<&Family> {
		for font in &self.fonts {
			if font.face == name.as_ref() {
				return Some(font);
			}
		}

		None
	}

	fn family_mut<S: AsRef<str>>(&mut self, name: S) -> Option<&mut Family> {
		for font in self.fonts.iter_mut() {
			if font.face == name.as_ref() {
				return Some(font);
			}
		}

		None
	}

	fn regular<S: AsRef<str>>(&self, fam: S) -> Option<Font> {
		self.varient(fam, "regular")
	}

	pub fn varient<F: AsRef<str>, V: AsRef<str>>(&self, family: F, varient: V) -> Option<Font> {
		if let Some(fam) = self.family(family.as_ref()) {
			if let Some(path) = fam.varient(varient.as_ref()) {
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
			let (family, varient) = match fname.rsplit_once('-') {
				Some((varient, family)) => (varient, family),
				_ => {
					eprintln!("Unknown file in cache: {}", fname);
					continue;
				}
			};

			let ftype = entry.file_type().unwrap();

			if ftype.is_file() {
				if let Some(fam) = self.family_mut(family) {
					fam.push(varient, entry.path().to_str().unwrap());
				} else {
					let mut fam = Family::new(family);
					fam.push(varient, entry.path().to_str().unwrap());
					self.fonts.push(fam);
				}
			}
		}

		println!("{} files in cache", self.fonts.len());

		Ok(())
	}

	fn save_font<F: AsRef<str>, V: AsRef<str>>(&mut self, family: F, varient: V, buf: &[u8]) {
		let family = family.as_ref();
		let varient = varient.as_ref();
		let fname = format!("{}-{}.ttf", family, varient);
		let mut path = self.location.clone();
		path.push(fname);

		let mut file = File::create(&path).unwrap();
		file.write_all(buf).unwrap();

		if let Some(family) = self.family_mut(family) {
			family.push(varient, path.to_str().unwrap())
		} else {
			let mut fam = Family::new(family);
			fam.push(varient, path.to_str().unwrap());
			self.fonts.push(fam);
		}

		println!("saved font {}", path.to_str().unwrap());
	}
}

pub struct FontProvider {
	default: Arc<Font>,
	fonts: Vec<Family>,
	font_cache: FontCache,
}

impl FontProvider {
	fn new<P: AsRef<Path>>(fontcache: P) -> Self {
		Self {
			default: Arc::new(
				fontster::parse_font(include_bytes!("../Cabin-Regular.ttf")).unwrap(),
			),
			fonts: vec![],
			font_cache: FontCache::new(fontcache.as_ref()).unwrap(),
		}
	}

	pub fn google<P: AsRef<Path>>(fontcache: P, apikey: &str) -> Result<FontProvider, ureq::Error> {
		let api_str = format!(
			"https://www.googleapis.com/webfonts/v1/webfonts?key={}",
			apikey
		);

		let before = Instant::now();
		let response = ureq::get(&api_str).call()?;
		let json: Value = serde_json::from_str(&response.into_string()?).unwrap();

		let fonts = match &json["items"] {
			Value::Array(fonts) => fonts,
			_ => panic!(),
		};

		let mut provider = FontProvider::new(fontcache);

		for item in fonts {
			let name = item["family"].as_str().unwrap();
			let mut family = Family::new(name);

			for (style, filepath) in item["files"].as_object().unwrap() {
				family.push(style, filepath.as_str().unwrap());
				println!("Font {} Varient {}", name, style);
			}

			provider.push(family);
		}
		println!(
			"getting font list took {}s",
			Instant::now().duration_since(before).as_secs()
		);

		Ok(provider)
	}

	pub fn cached(&self) -> usize {
		self.font_cache
			.fonts
			.iter()
			.fold(0, |acc, fam| acc + fam.varients.len())
	}

	fn push(&mut self, fam: Family) {
		self.fonts.push(fam);
	}

	fn family<S: AsRef<str>>(&self, face: S) -> Option<&Family> {
		for font in &self.fonts {
			if font.face == face.as_ref() {
				return Some(font);
			}
		}

		None
	}

	pub fn varient<F: AsRef<str>, V: AsRef<str>>(&mut self, family: F, varient: V) -> Arc<Font> {
		if let Some(font) = self.font_cache.varient(family.as_ref(), varient.as_ref()) {
			println!(
				"hit cache for {} varient {}",
				family.as_ref(),
				varient.as_ref()
			);

			return Arc::new(font);
		} else if self.family(family.as_ref()).is_some() {
			println!(
				"missed cache for {} varient {}",
				family.as_ref(),
				varient.as_ref()
			);

			if let Some(var) = self
				.family(family.as_ref())
				.unwrap()
				.varient(varient.as_ref())
				.map(|s| s.to_owned())
			{
				let response = ureq::get(&var).call().unwrap();
				let mut buffer: Vec<u8> = Vec::new();
				response.into_reader().read_to_end(&mut buffer).unwrap();
				self.font_cache.save_font(family, varient, &buffer);

				return Arc::new(fontster::parse_font(&buffer).unwrap());
			}
		}

		self.default.clone()
	}

	pub fn regular<S: AsRef<str>>(&mut self, fam: S) -> Arc<Font> {
		self.varient(fam, "regular")
	}

	pub fn default_font(&self) -> Arc<Font> {
		self.default.clone()
	}
}

struct Family {
	face: String,
	varients: Vec<(String, String)>,
}

impl Family {
	fn new<S: Into<String>>(face: S) -> Self {
		Family {
			face: face.into(),
			varients: vec![],
		}
	}

	fn push<V: Into<String>, P: Into<String>>(&mut self, varient: V, path: P) {
		self.varients.push((varient.into(), path.into()));
	}

	fn varient<S: AsRef<str>>(&self, name: S) -> Option<&str> {
		for (varient, path) in &self.varients {
			if varient == name.as_ref() {
				return Some(path);
			}
		}

		None
	}
}

/// Font weight names. List taken from here: https://en.wikipedia.org/wiki/Font#Weight
pub enum FontWeight {
	Thin,
	ExtraLight,
	Light,
	Normal,
	Medium,
	SemiBold,
	Bold,
	ExtraBold,
	Heavy,
	ExtraBlack,
}

impl FontWeight {
	pub fn into_weight_number(&self) -> usize {
		match self {
			FontWeight::Thin => 100,
			FontWeight::ExtraLight => 200,
			FontWeight::Light => 300,
			FontWeight::Normal => 400,
			FontWeight::Medium => 500,
			FontWeight::SemiBold => 600,
			FontWeight::Bold => 700,
			FontWeight::ExtraBold => 800,
			FontWeight::Heavy => 900,
			FontWeight::ExtraBlack => 950,
		}
	}
}

impl FromStr for FontWeight {
	type Err = FontWeightParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		// gen- the urge to panic on "book" is high, but BUT! I will not.
		match s.to_lowercase().as_str() {
			"thin" | "100" => Ok(FontWeight::Thin),
			"extralight" | "extra-light" | "ultralight" | "ultra-light" | "200" => {
				Ok(FontWeight::ExtraLight)
			}
			"light" | "300" => Ok(FontWeight::Light),
			"normal" | "regular" | "400" => Ok(FontWeight::Normal),
			"medium" | "500" => Ok(FontWeight::Medium),
			"semibold" | "semi-bold" | "demibold" | "demi-bold" | "600" => Ok(FontWeight::SemiBold),
			"bold" | "700" => Ok(FontWeight::Bold),
			"extrabold" | "extra-bold" | "ultrabold" | "ultra-bold" | "800" => {
				Ok(FontWeight::ExtraBold)
			}
			"heavy" | "900" => Ok(FontWeight::Heavy),
			"extrablack" | "extra-black" | "ultrablack" | "ultra-black" | "950" => {
				Ok(FontWeight::ExtraBlack)
			}
			_ => Err(FontWeightParseError::UnknownWeightName {
				weight: s.to_owned(),
			}),
		}
	}
}

#[derive(Debug, Error)]
pub enum FontWeightParseError {
	#[error("The weight {weight} is not recognised")]
	UnknownWeightName { weight: String },
}
