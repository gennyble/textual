use std::sync::Arc;

use bracket_color::prelude::RGB as BrRGB;
use fontster::{Font, Settings};
use image::jpeg::JpegEncoder;
use serde_derive::{Deserialize, Serialize};
use warp::{
    hyper::{Response, StatusCode},
    Filter,
};

#[derive(Clone, Deserialize, Serialize)]
struct Query {
    text: String,
    color: Option<String>,
    bcolor: Option<String>,
}

impl Into<Settings> for Query {
    fn into(self) -> Settings {
        let colordef = |cstropt: Option<String>, def: (u8, u8, u8)| match cstropt {
            Some(color_string) => {
                if let Ok(color) = BrRGB::from_hex(format!("#{}", color_string)) {
                    (
                        (color.r * 255.0) as u8,
                        (color.g * 255.0) as u8,
                        (color.b * 255.0) as u8,
                    )
                } else {
                    def
                }
            }
            None => def,
        };

        Settings {
            text_color: colordef(self.color, (255, 255, 255)),
            background_color: colordef(self.bcolor, (0, 0, 0)),
            draw_baseline: false,
            draw_glyph_outline: false,
            draw_sentence_outline: false,
        }
    }
}

#[tokio::main]
async fn main() {
    let font = Arc::new(fontster::get_font());

    let opt_query = warp::query::<Query>()
        .map(Some)
        .or_else(|_| async { Ok::<(Option<Query>,), std::convert::Infallible>((None,)) });

    let text_image =
        warp::get()
            .and(warp::path::end())
            .and(opt_query)
            .map(move |p: Option<Query>| match p {
                Some(query) => {
                    let buffer = make_image(font.as_ref(), query);

                    Response::builder()
                        .header("Content-type", "image/jpeg")
                        .body(buffer)
                }
                None => Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(b"NO!".as_ref().to_owned()),
            });

    warp::serve(text_image).run(([127, 0, 0, 1], 8080)).await
}

fn make_image(font: &Font, query: Query) -> Vec<u8> {
    let image = fontster::do_sentence(font, &query.text, query.clone().into());
    let mut encoded_buffer = vec![];

    let mut encoder = JpegEncoder::new(&mut encoded_buffer);
    encoder
        .encode(
            image.data(),
            image.width() as u32,
            image.height() as u32,
            image::ColorType::Rgb8,
        )
        .unwrap();

    encoded_buffer
}
