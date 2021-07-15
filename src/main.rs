extern crate image as crateimage;

mod color;
mod fontprovider;
mod image;
mod query;
mod text;

use std::{
    cell::Cell,
    convert::TryInto,
    net::{TcpListener, TcpStream},
};

use crateimage::png::PngEncoder;
use fontprovider::FontProvider;
use query::QueryParseError;
use serde::Serialize;
use small_http::{Connection, ConnectionError, Response};
use smol::{
    lock::{Mutex, RwLock},
    Async,
};
use std::sync::Arc;
use text::{Text, TextError};
use thiserror::Error;
use tinytemplate::TinyTemplate;

use crate::query::Query;

#[derive(Debug, Default)]
struct Statistics {
    image_bytes_sent: usize,
    html_bytes_sent: usize,
}

struct Textual {
    statistics: RwLock<Statistics>,
    font_provider: RwLock<FontProvider>,
}

fn main() {
    let provider = FontProvider::google().unwrap();
    let textual = Textual {
        font_provider: RwLock::new(provider),
        statistics: RwLock::new(Statistics::default()),
    };

    let listener = Async::<TcpListener>::bind(([127, 0, 0, 1], 8080)).unwrap();

    smol::block_on(listen(Arc::new(textual), listener))
}

async fn listen(textual: Arc<Textual>, listener: Async<TcpListener>) {
    loop {
        let (stream, clientaddr) = listener.accept().await.unwrap();
        println!("connection from {}", clientaddr);

        let task = smol::spawn(error_handler(textual.clone(), stream));
        task.detach();
    }
}

async fn error_handler(textual: Arc<Textual>, stream: Async<TcpStream>) {
    let mut connection = Connection::new(stream);

    let response = match serve(textual, &mut connection).await {
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
    textual: Arc<Textual>,
    con: &mut Connection,
) -> Result<Response<Vec<u8>>, ServiceError> {
    let request = con.request().await?.unwrap();

    let query = match request.uri().query() {
        Some(query_str) => query_str,
        None => return Ok(serve_tool()?),
    };

    println!("'{}'", query);

    let mut text: Text = query.parse::<Query>()?.try_into()?;

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

    let scheme = request.uri().scheme_str().unwrap_or("http");
    let host = request.headers().get("host").unwrap().to_str().unwrap();

    if text.info {
        let stats = textual.statistics.read().await;
        let provider = textual.font_provider.read().await;

        text = Text {
            text: format!(
                "image bytes sent: {}\nhtml bytse sent: {}\nfonts in cache: {}",
                bytes_to_human(stats.image_bytes_sent),
                bytes_to_human(stats.html_bytes_sent),
                provider.cached()
            ),
            forceraw: text.forceraw,
            ..Default::default()
        };
    }

    if text.forceraw {
        // Image
        Ok(make_image(textual, text).await?)
    } else {
        let link = format!("{}://{}?{}&forceraw", scheme, host, query);

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
    text: Text,
) -> Result<Response<Vec<u8>>, ConnectionError> {
    let image = text.make_image(&textual.font_provider).await;

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

    {
        let mut stats = textual.statistics.write().await;
        stats.image_bytes_sent += encoded_buffer.len();
    }

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
    font: String,
    hex_color: String,
}

async fn make_meta(
    textual: Arc<Textual>,
    text: Text,
    link: String,
) -> Result<Response<Vec<u8>>, ConnectionError> {
    let mut tt = TinyTemplate::new();
    tt.add_template("html", TEMPLATE).unwrap();

    let content = Meta {
        text: text.text.clone(),
        image_link: link,
        font: text.font.clone().unwrap_or("Cabin".into()).clone(),
        hex_color: text.color.as_hex()[..6].into(),
    };

    let doc = tt.render("html", &content).unwrap();
    let buffer = doc.as_bytes().to_vec();

    {
        let mut stats = textual.statistics.write().await;
        stats.image_bytes_sent += buffer.len();
    }

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
