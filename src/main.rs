mod query;
mod text;

use std::{
    convert::{TryFrom, TryInto},
    net::{TcpListener, TcpStream},
};

use fontster::{Color, Font, Settings};
use image::png::PngEncoder;
use query::QueryParseError;
use small_http::{Connection, ConnectionError, Response};
use smol::Async;
use std::sync::Arc;
use text::TextError;
use thiserror::Error;

use crate::query::Query;
use crate::text::Text;

fn main() {
    let listener = Async::<TcpListener>::bind(([127, 0, 0, 1], 8080)).unwrap();

    smol::block_on(listen(listener))
}

async fn listen(listener: Async<TcpListener>) {
    let font = Arc::new(vec![fontster::get_font(), fontster::get_font_italic()]);

    loop {
        let (stream, clientaddr) = listener.accept().await.unwrap();
        println!("connection from {}", clientaddr);

        let task = smol::spawn(error_handler(font.clone(), stream));
        task.detach();
    }
}

async fn error_handler(arcfont: Arc<Vec<Font>>, stream: Async<TcpStream>) {
    let mut connection = Connection::new(stream);

    let response = match serve(arcfont, &mut connection).await {
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
    arcfont: Arc<Vec<Font>>,
    con: &mut Connection,
) -> Result<Response<Vec<u8>>, ServiceError> {
    let request = con.parse_request().await?;
    let query: Query = request.uri().query().unwrap_or("").parse()?;
    let mut text: Text = query.try_into()?;

    let agent = request
        .headers()
        .get("user-agent")
        .unwrap()
        .to_str()
        .unwrap();
    println!("ua: {:?}", request.headers().get("user-agent"));

    let img = if agent.contains("Discordbot")
        || agent.trim()
            == "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.10; rv:38.0) Gecko/20100101 Firefox/38.0"
    {
        //text.text = String::from("Discordbot");
        make_image(arcfont.as_ref(), text)
    } else {
        make_image(arcfont.as_ref(), text)
        //return Ok(Response::builder().status(500).body(vec![]).unwrap());
    };

    Ok(Response::builder()
        .header("content-type", "image/png")
        .header("content-length", img.len())
        .body(img)
        .map_err(|e| ConnectionError::UnknownError(e))?)
}

fn make_image(font: &Vec<Font>, text: Text) -> Vec<u8> {
    let font = if text.italic { &font[1] } else { &font[0] };

    let image = fontster::do_sentence(font, &text.text, text.clone().into());
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

    encoded_buffer
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
