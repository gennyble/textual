use std::{
    io::{self, Read, Write},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

use fontster::Font;
use serde_json::Value;
use std::fs::File;

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
            if font.name == name.as_ref() {
                return Some(font);
            }
        }

        None
    }

    fn family_mut<S: AsRef<str>>(&mut self, name: S) -> Option<&mut Family> {
        for font in self.fonts.iter_mut() {
            if font.name == name.as_ref() {
                return Some(font);
            }
        }

        None
    }

    fn get_regular<S: AsRef<str>>(&self, fam: S) -> Option<Font> {
        if let Some(fam) = self.family(fam) {
            if let Some(path) = fam.varient("regular") {
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
    fn new() -> Self {
        Self {
            default: Arc::new(
                fontster::parse_font(include_bytes!("../Cabin-Regular.ttf")).unwrap(),
            ),
            fonts: vec![],
            font_cache: FontCache::new("fonts").unwrap(),
        }
    }

    pub fn google() -> Result<FontProvider, ureq::Error> {
        let api_str = format!(
            "https://www.googleapis.com/webfonts/v1/webfonts?key={}",
            include_str!("webfont.key")
        );

        let before = Instant::now();
        let response = ureq::get(&api_str).call()?;
        let json: Value = serde_json::from_str(&response.into_string()?).unwrap();

        let fonts = match &json["items"] {
            Value::Array(fonts) => fonts,
            _ => panic!(),
        };

        let mut provider = FontProvider::new();

        for item in fonts {
            let name = item["family"].as_str().unwrap();
            let mut family = Family::new(name);

            for (style, filepath) in item["files"].as_object().unwrap() {
                family.push(style, filepath.as_str().unwrap());
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
        self.font_cache.fonts.len()
    }

    fn push(&mut self, fam: Family) {
        self.fonts.push(fam);
    }

    fn family<S: AsRef<str>>(&self, name: S) -> Option<&Family> {
        for font in &self.fonts {
            if font.name == name.as_ref() {
                return Some(font);
            }
        }

        None
    }

    pub fn regular<S: AsRef<str>>(&mut self, fam: Option<S>) -> Arc<Font> {
        if let Some(fam) = fam {
            let fam = fam.as_ref();

            if let Some(font) = self.font_cache.get_regular(fam) {
                println!("hit cache for {}", fam);
                return Arc::new(font);
            } else if let Some(family) = self.family(fam) {
                println!("missed cache for {}", fam);

                let regular = family.varient("regular").unwrap();
                let response = ureq::get(regular).call().unwrap();
                let mut buffer: Vec<u8> = Vec::new();
                response.into_reader().read_to_end(&mut buffer).unwrap();

                self.font_cache.save_font(fam, "regular", &buffer);

                return Arc::new(fontster::parse_font(&buffer).unwrap());
            }
        }

        self.default.clone()
    }
}

struct Family {
    name: String,
    varients: Vec<(String, String)>,
}

impl Family {
    fn new<S: Into<String>>(name: S) -> Self {
        Family {
            name: name.into(),
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
