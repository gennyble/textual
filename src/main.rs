mod query;
mod text;

use std::{
    convert::{TryFrom, TryInto},
    io::Read,
    net::{TcpListener, TcpStream},
    time::Instant,
};

use fontster::{Color, Font, Settings};
use image::png::PngEncoder;
use query::QueryParseError;
use serde::Serialize;
use serde_json::Value;
use small_http::{Connection, ConnectionError, Response};
use smol::Async;
use std::sync::Arc;
use text::TextError;
use thiserror::Error;
use tinytemplate::TinyTemplate;

use crate::query::Query;
use crate::text::Text;

struct FontProvider {
    default: Arc<Font>,
    fonts: Vec<Family>,
}

impl FontProvider {
    fn new() -> Self {
        Self {
            default: Arc::new(fontster::get_font()),
            fonts: vec![],
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

    fn regular<S: AsRef<str>>(&self, fam: Option<S>) -> Arc<Font> {
        if let Some(fam) = fam {
            if let Some(family) = self.family(fam) {
                let regular = family.varient("regular").unwrap();
                let response = ureq::get(regular).call().unwrap();
                let mut buffer: Vec<u8> = Vec::new();
                let body = response.into_reader().read_to_end(&mut buffer).unwrap();
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

fn main() {
    let provider = get_font_list().unwrap();

    let listener = Async::<TcpListener>::bind(([127, 0, 0, 1], 8080)).unwrap();

    smol::block_on(listen(Arc::new(provider), listener))
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

async fn listen(fp: Arc<FontProvider>, listener: Async<TcpListener>) {
    loop {
        let (stream, clientaddr) = listener.accept().await.unwrap();
        println!("connection from {}", clientaddr);

        let task = smol::spawn(error_handler(fp.clone(), stream));
        task.detach();
    }
}

async fn error_handler(provider: Arc<FontProvider>, stream: Async<TcpStream>) {
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
    provider: Arc<FontProvider>,
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

    let font = provider.regular(text.font.clone());

    if text.forceraw {
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
