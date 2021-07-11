mod query;
mod text;

use std::{
    convert::{TryFrom, TryInto},
    fs::{File, FileType},
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    time::Instant,
};

use fontster::{Color, Font, Settings};
use image::png::PngEncoder;
use query::QueryParseError;
use serde::Serialize;
use serde_json::Value;
use small_http::{Connection, ConnectionError, Response};
use smol::{lock::Mutex, Async};
use std::sync::Arc;
use text::TextError;
use thiserror::Error;
use tinytemplate::TinyTemplate;

use crate::query::Query;
use crate::text::Text;

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

                return Some(fontster::parse_font(&mut buffer));
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
            let (varient, family) = match fname.split_once('-') {
                Some((varient, family)) => (varient, family),
                _ => {
                    eprintln!("Unknown file in cache: {}", fname);
                    continue;
                }
            };

            let ftype = entry.file_type().unwrap();

            if ftype.is_file() {
                if let Some(mut fam) = self.family_mut(family) {
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
        let fname = format!("{}-{}.ttf", varient, family);
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

struct FontProvider {
    default: Arc<Font>,
    fonts: Vec<Family>,
    font_cache: FontCache,
}

impl FontProvider {
    fn new() -> Self {
        Self {
            default: Arc::new(fontster::get_font()),
            fonts: vec![],
            font_cache: FontCache::new("/tmp/fonts").unwrap(),
        }
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

    fn regular<S: AsRef<str>>(&mut self, fam: Option<S>) -> Arc<Font> {
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
                let body = response.into_reader().read_to_end(&mut buffer).unwrap();

                self.font_cache.save_font(fam, "regular", &buffer);

                return Arc::new(fontster::parse_font(&buffer));
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

type Provider = Arc<Mutex<FontProvider>>;

fn main() {
    let provider = get_font_list().unwrap();

    let listener = Async::<TcpListener>::bind(([127, 0, 0, 1], 8080)).unwrap();

    smol::block_on(listen(Arc::new(Mutex::new(provider)), listener))
}

fn get_font_list() -> Result<FontProvider, ureq::Error> {
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

async fn listen(fp: Provider, listener: Async<TcpListener>) {
    loop {
        let (stream, clientaddr) = listener.accept().await.unwrap();
        println!("connection from {}", clientaddr);

        let task = smol::spawn(error_handler(fp.clone(), stream));
        task.detach();
    }
}

async fn error_handler(provider: Provider, stream: Async<TcpStream>) {
    let mut connection = Connection::new(stream);

    let response = match serve(provider, &mut connection).await {
        Ok(resp) => resp,
        Err(e) => Response::builder()
            .header("content-type", "text/plain")
            .body(e.to_string().as_bytes().to_vec())
            .unwrap(),
    };

    connection
        .respond(response)
        .await
        .expect("Failed to respond to connection")
}

async fn serve(
    provider: Arc<Mutex<FontProvider>>,
    con: &mut Connection,
) -> Result<Response<Vec<u8>>, ServiceError> {
    let request = con.parse_request().await?;
    let query_string = request.uri().query().unwrap_or("");
    let mut text: Text = query_string.parse::<Query>()?.try_into()?;

    let agent = request
        .headers()
        .get("user-agent")
        .unwrap()
        .to_str()
        .unwrap();
    println!(
        "ua: {}\n\tpath: {}",
        agent,
        request.uri().path_and_query().unwrap().to_string()
    );

    if text.forceraw {
        let font = {
            let mut provider = provider.lock().await;
            provider.regular(text.font.clone())
        };
        // Image
        Ok(make_image(font, text)?)
    } else {
        Ok(make_meta(query_string, text)?)
    }
}

fn make_image(font: Arc<Font>, text: Text) -> Result<Response<Vec<u8>>, ConnectionError> {
    let image = fontster::do_sentence(font.as_ref(), &text.text, text.clone().into());
    let mut encoded_buffer = vec![];

    let mut encoder = PngEncoder::new(&mut encoded_buffer);
    encoder
        .encode(
            image.data(),
            image.width() as u32,
            image.height() as u32,
            image::ColorType::Rgba8,
        )
        .unwrap();

    Response::builder()
        .header("content-type", "image/png")
        .header("content-length", encoded_buffer.len())
        .body(encoded_buffer)
        .map_err(|e| ConnectionError::UnknownError(e))
}

static TEMPLATE: &'static str = include_str!("template.htm");

#[derive(Debug, Serialize)]
struct Meta {
    text: String,
    image_link: String,
}

fn make_meta<S: AsRef<str>>(
    query_string: S,
    text: Text,
) -> Result<Response<Vec<u8>>, ConnectionError> {
    let mut tt = TinyTemplate::new();
    tt.add_template("html", TEMPLATE).unwrap();

    let content = Meta {
        text: text.text,
        image_link: format!(
            "https://textual.bookcase.name?{}&forceraw",
            query_string.as_ref()
        ),
    };

    let doc = tt.render("html", &content).unwrap();
    let buffer = doc.as_bytes().to_vec();

    Response::builder()
        .header("content-type", "text/html")
        .header("content-length", buffer.len())
        .body(buffer)
        .map_err(|e| ConnectionError::UnknownError(e))
}

#[derive(Debug, Error)]
enum ServiceError {
    #[error("{0}")]
    ClientError(#[from] ConnectionError),
    #[error("your query string did not make sense: {0}")]
    QueryError(#[from] QueryParseError),
    #[error("{0}")]
    TextError(#[from] TextError),
}
