extern crate image as crateimage;

mod color;
mod config;
mod fontprovider;
mod image;
mod statistics;
mod text;

use std::{
    cell::Cell,
    collections::HashMap,
    convert::TryInto,
    net::{TcpListener, TcpStream},
    time::Instant,
};

use chrono::Utc;
use crateimage::png::PngEncoder;
use fontprovider::FontProvider;
use serde::Serialize;
use small_http::{Connection, ConnectionError, Query, QueryParseError, Response};
use smol::{
    lock::{Mutex, RwLock},
    Async,
};
use std::sync::Arc;
use text::{Operation, Text};
use thiserror::Error;
use tinytemplate::TinyTemplate;

use crate::config::Config;
use crate::statistics::Statistics;

struct Textual {
    config: Config,
    statistics: RwLock<Statistics>,
    font_provider: RwLock<FontProvider>,
}

fn main() {
    let config = match Config::get() {
        Ok(Some(c)) => c,
        Ok(None) => return,
        Err(e) => {
            println!("{}", e);
            std::process::exit(1);
        }
    };

    let provider =
        FontProvider::google(config.font_cache_path(), include_str!("webfont.key")).unwrap();

    let textual = Textual {
        config,
        font_provider: RwLock::new(provider),
        statistics: RwLock::new(Statistics::default()),
    };

    let listener =
        Async::<TcpListener>::bind((textual.config.listen(), textual.config.port())).unwrap();

    smol::block_on(listen(Arc::new(textual), listener))
}

async fn listen(textual: Arc<Textual>, listener: Async<TcpListener>) {
    loop {
        let (stream, _clientaddr) = listener.accept().await.unwrap();
        let task = smol::spawn(error_handler(textual.clone(), stream));
        task.detach();
    }
}

async fn error_handler(textual: Arc<Textual>, stream: Async<TcpStream>) {
    let mut connection = Connection::new(stream);

    let response = match serve(textual.clone(), &mut connection).await {
        Ok(resp) => resp,
        Err(e) => Response::builder()
            .header("content-type", "text/plain")
            .body(e.to_string().as_bytes().to_vec())
            .unwrap(),
    };

    match response.headers().get("content-type").map(|hv| hv.to_str()) {
        Some(Ok(mime)) => {
            let mut stats = textual.statistics.write().await;
            stats.add(mime, response.body().len());
        }
        _ => {
            //TODO: Maybe print here that the content-type was unset or could not be parsed as a string
        }
    }

    connection
        .respond(response)
        .await
        .expect("Failed to respond to connection")
}

async fn serve(
    textual: Arc<Textual>,
    con: &mut Connection,
) -> Result<Response<Vec<u8>>, ServiceError> {
    let request = con.request().await?.unwrap();

    // Return the tool page if the query string is empty or not there at all
    let mut query_str = match request.uri().query() {
        None => return Ok(serve_tool()?),
        Some(query_str) if query_str.is_empty() => return Ok(serve_tool()?),
        Some(query_str) => query_str.to_owned(),
    };

    let query: Query = query_str.parse()?;

    // if we have `info` and `forceraw` then we should pass this by; we're
    // generating the `info` image itself if that's the case
    if query.has_bool("info") && !query.has_bool("forceraw") {
        let stats = textual.statistics.read().await;
        let provider = textual.font_provider.read().await;

        let text = format!(
            "{}\n\nimage sent: {}\nhtml sent: {}\nfonts in cache: {}",
            Utc::now().format("%H:%M UTC\n%a %B %-d %Y"),
            bytes_to_human(stats.image()),
            bytes_to_human(stats.html()),
            provider.cached()
        );

        let font = match query.get_first_value("font") {
            Some(font) => format!("font={}&", font),
            None => String::new(),
        };

        query_str = format!(
            "{}fs=32&c=black&bc=eed&lh=font&text={}",
            font,
            Query::url_encode(&text)
        );
    }

    let agent = request
        .headers()
        .get("user-agent")
        .unwrap()
        .to_str()
        .unwrap();

    let clientaddr = request
        .headers()
        .get("X-Forwarded-For")
        .map(|h| h.to_str().unwrap_or("unknown"))
        .unwrap_or("unknown");

    println!(
        "connection: {}\n\tua: {}\n\tpath: {}",
        clientaddr,
        agent,
        request.uri().path_and_query().unwrap().to_string()
    );

    if query.has_bool("me") && !query.has_bool("forceraw") && !query.has_bool("info") {
        let text = format!("IP: {}\n\nUser Agent\n{}", clientaddr, agent);

        let font = match query.get_first_value("font") {
            Some(font) => format!("font={}&", font),
            None => String::new(),
        };

        query_str = format!(
            "{}fs=32&c=black&bc=eed&lh=font&text={}",
            font,
            Query::url_encode(&text)
        );
    }

    let text: Operation = query.into();

    // Find the hostname we should use for the image link in the opengraph tags
    let host = textual
        .config
        .meta_host()
        .or(request
            .headers()
            .get("host")
            .map(|hv| hv.to_str().ok())
            .flatten())
        .unwrap_or("localhost");

    let scheme = textual
        .config
        .scheme()
        .or(request.uri().scheme_str())
        .unwrap_or(if host == "localhost" { "http" } else { "https" });

    if text.forceraw {
        // Image
        Ok(make_image(textual, text).await?)
    } else {
        let link = format!("{}://{}?{}&forceraw", scheme, host, query_str);
        Ok(make_meta(textual, text, link).await?)
    }
}

fn serve_tool() -> Result<Response<Vec<u8>>, ConnectionError> {
    let html = std::fs::read_to_string("tool.html")?.into_bytes();

    Response::builder()
        .header("content-type", "text/html")
        .header("content-length", html.len())
        .body(html)
        .map_err(|e| ConnectionError::UnknownError(e))
}

fn bytes_to_human(bytes: usize) -> String {
    let mut bytes = bytes as f32;
    let mut suffix = "B";

    if bytes >= 1024.0 {
        bytes /= 1024.0;
        suffix = "KB";
    }

    if bytes >= 1024.0 {
        bytes /= 1024.0;
        suffix = "MB";
    }

    if bytes >= 1024.0 {
        bytes /= 1024.0;
        suffix = "GB";
    }

    format!("{} {}", (bytes * 10.0).ceil() / 10.0, suffix)
}

async fn make_image(
    textual: Arc<Textual>,
    op: Operation,
) -> Result<Response<Vec<u8>>, ConnectionError> {
    let image = op.make_image(&textual.font_provider).await;

    let mut encoded_buffer = vec![];

    let encoder = PngEncoder::new(&mut encoded_buffer);
    encoder
        .encode(
            image.data(),
            image.width() as u32,
            image.height() as u32,
            crateimage::ColorType::Rgba8,
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
    twitter_image: String,
    og_image: String,
    image: String,
    font: String,
    hex_color: String,
}

async fn make_meta(
    textual: Arc<Textual>,
    op: Operation,
    link: String,
) -> Result<Response<Vec<u8>>, ConnectionError> {
    let buffer = {
        let mut tt = TinyTemplate::new();
        tt.add_template("html", TEMPLATE).unwrap();

        let content = Meta {
            text: op.full_text(),
            twitter_image: format!("{}&aspect=1.8", link),
            og_image: format!("{}&aspect=1.8", link),
            image: link,
            font: String::new(),
            hex_color: String::new(),
        };

        tt.render("html", &content).unwrap().into_bytes()
    };

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
}
