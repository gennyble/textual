use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use common::{FontStyle, FontVariant};
use serde_json::Value;
use std::fs::File;

pub struct FontFamily {
    pub face: String,
    pub variants: Vec<(FontVariant, String)>,
}

impl FontFamily {
    pub fn new<S: Into<String>>(face: S) -> Self {
        FontFamily {
            face: face.into(),
            variants: vec![],
        }
    }

    pub fn push<P: Into<String>>(&mut self, variant: FontVariant, path: P) {
        self.variants.push((variant, path.into()));
    }

    /// Could be a filepath or a URL depending on how you're using this.
    /// FontProvider stores URLs, FontCache local files
    pub fn variant_path(&self, variant: FontVariant) -> Option<&String> {
        for (our_varient, path) in &self.variants {
            if *our_varient == variant {
                return Some(path);
            }
        }

        None
    }
}

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
        self.fonts.iter().find(|f| f.face == name.as_ref())
    }

    fn family_mut<S: AsRef<str>>(&mut self, name: S) -> Option<&mut FontFamily> {
        self.fonts.iter_mut().find(|f| f.face == name.as_ref())
    }

    fn regular<S: AsRef<str>>(&self, fam: S) -> Option<Vec<u8>> {
        self.variant(fam, FontVariant::default())
    }

    pub fn variant<F: AsRef<str>>(&self, family: F, variant: FontVariant) -> Option<Vec<u8>> {
        if let Some(fam) = self.family(family.as_ref()) {
            if let Some(path) = fam.variant_path(variant) {
                let mut file = File::open(path).unwrap();

                let mut buffer = vec![];
                file.read_to_end(&mut buffer).unwrap();

                return Some(buffer);
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
    //default: Arc<Font>,
    fonts: Vec<FontFamily>,
    font_cache: FontCache,
}

impl FontProvider {
    pub fn new<P: AsRef<Path>>(fontcache: P) -> Self {
        let google = get_fonts_from_google().unwrap();

        Self {
            /*default: Arc::new(
                fontster::parse_font(include_bytes!("../Cabin-Regular.ttf")).unwrap(),
            ),*/
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

    pub fn variant_cached<F: Into<String>>(&self, family: F, variant: FontVariant) -> CachedFont {
        let family_string = family.into();

        if let Some(font) = self.font_cache.variant(&family_string, variant) {
            return CachedFont::Available { font };
        } else if let Some(family) = self.family(&family_string) {
            if family.variant_path(variant).is_some() {
                return CachedFont::Known;
            }
        }

        CachedFont::Unknown
    }

    pub fn variant<F: Into<String>>(&mut self, family: F, variant: FontVariant) -> Option<Vec<u8>> {
        let family_string = family.into();

        if let Some(font) = self.font_cache.variant(&family_string, variant) {
            println!("hit cache for {} {}", family_string, variant);

            return Some(font);
        } else if let Some(family) = self.family(&family_string) {
            println!("missed cache for {} {}", family_string, variant);

            if let Some(var) = family.variant_path(variant).map(<_>::to_owned) {
                let response = ureq::get(&var).call().unwrap();

                let mut buffer: Vec<u8> = Vec::new();
                response.into_reader().read_to_end(&mut buffer).unwrap();

                self.font_cache.save_font(family_string, variant, &buffer);

                return Some(buffer);
            }
        }

        None
    }

    pub fn regular<S: AsRef<str>>(&mut self, fam: S) -> Option<Vec<u8>> {
        self.variant(fam.as_ref(), FontVariant::default())
    }
}

pub enum CachedFont {
    /// We have it in the cache, here it is
    Available { font: Vec<u8> },
    /// It's not cached, but it exists
    Known,
    /// What are you on about?
    Unknown,
}

fn get_fonts_from_google() -> Result<Vec<FontFamily>, ureq::Error> {
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
