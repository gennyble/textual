extern crate image as crateimage;

mod color;
mod fontprovider;
mod image;
mod query;
mod text;

use std::{
    convert::TryInto,
    net::{TcpListener, TcpStream},
};

use crateimage::png::PngEncoder;
use fontprovider::FontProvider;
use query::QueryParseError;
use serde::Serialize;
use small_http::{Connection, ConnectionError, Response};
use smol::{lock::Mutex, Async};
use std::sync::Arc;
use text::{Text, TextError};
use thiserror::Error;
use tinytemplate::TinyTemplate;

use crate::query::Query;

type Provider = Arc<Mutex<FontProvider>>;

fn main() {
    let provider = FontProvider::google().unwrap();

    let listener = Async::<TcpListener>::bind(([127, 0, 0, 1], 8080)).unwrap();

    smol::block_on(listen(Arc::new(Mutex::new(provider)), listener))
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
    let request = con.request().await?.unwrap();

    let query = match request.uri().query() {
        Some(query_str) => query_str,
        None => return Ok(serve_tool()?),
    };

    println!("'{}'", query);

    let text: Text = query.parse::<Query>()?.try_into()?;

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
        // Image
        Ok(make_image(provider, text).await?)
    } else {
        Ok(make_meta(query, text)?)
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

async fn make_image(
    mut provider: Arc<Mutex<FontProvider>>,
    text: Text,
) -> Result<Response<Vec<u8>>, ConnectionError> {
    let image = text.make_image(&mut provider).await;

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
    image_link: String,
    font: String,
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
        font: text.font.unwrap_or("Cabin".into()),
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
