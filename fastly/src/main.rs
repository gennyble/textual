//! Default Compute@Edge template program.

use std::sync::OnceLock;

use anyhow::anyhow;
use fastly::http::{header, Method, StatusCode};
use fastly::{mime, Error, Request, Response};
use fontster::{Font, Layout, LayoutSettings, StyledText};
use png::{BitDepth, ColorType, Encoder};

const DOSIS_BYTES: &[u8] = include_bytes!("../Dosis-regular.otf");
static DOSIS: OnceLock<Font> = OnceLock::new();

/// The entry point for your application.
///
/// This function is triggered when your service receives a client request. It could be used to
/// route based on the request properties (such as method or path), send the request to a backend,
/// make completely new requests, and/or generate synthetic responses.
///
/// If `main` returns an error, a 500 error response will be delivered to the client.

#[fastly::main]
fn main(req: Request) -> Result<Response, Error> {
	// Log service version
	println!(
		"FASTLY_SERVICE_VERSION: {}",
		std::env::var("FASTLY_SERVICE_VERSION").unwrap_or_else(|_| String::new())
	);

	// Filter request methods...
	match req.get_method() {
		// Block requests with unexpected methods
		&Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE => {
			return Ok(Response::from_status(StatusCode::METHOD_NOT_ALLOWED)
				.with_header(header::ALLOW, "GET, HEAD, PURGE")
				.with_body_text_plain("This method is not allowed\n"))
		}

		// Let any other requests through
		_ => (),
	};

	let mut splits = req.get_path().split('/').skip(1);
	let family = splits.next().ok_or(anyhow!("no font family"))?;
	let style = splits.next().unwrap_or("normal");
	let weight = splits.next().unwrap_or("regular");

	let mut backres = Request::get(format!(
		"https://fonts.nyble.dev/font/{family}/{style}/{weight}"
	))
	.send("textual_fonts")?;

	if req.get_query_parameter("passthrough").is_some() {
		println!("passing through");
		return Ok(backres);
	}

	let font_bytes = backres.take_body().into_bytes();

	let text = req
		.get_query_parameter("text")
		.ok_or(anyhow!("what text do i draw?"))?;

	let img = layout_image(&font_bytes, text);

	let mut buf = vec![];
	let mut enc = Encoder::new(&mut buf, img.width as u32, img.height as u32);
	enc.set_color(ColorType::Grayscale);
	enc.set_depth(BitDepth::Eight);
	enc.write_header()?.write_image_data(&img.data)?;

	// Send a default synthetic response.
	Ok(Response::from_status(StatusCode::OK)
		.with_content_type(mime::IMAGE_PNG)
		.with_body(buf))
}

fn get_font() -> &'static Font {
	DOSIS.get_or_init(|| fontster::parse_font(DOSIS_BYTES).unwrap())
}

struct Image {
	width: usize,
	height: usize,
	data: Vec<u8>,
}

fn layout_image(font_bytes: &[u8], text: &str) -> Image {
	let font = fontster::parse_font(font_bytes).unwrap();
	let mut layout = Layout::<()>::new(LayoutSettings::default());
	layout.append(
		&[&font],
		StyledText {
			font_index: 0,
			font_size: 40.0,
			text,
			user: (),
		},
	);

	let width = layout.width().ceil() as usize + 32;
	let height = layout.height().ceil() as usize + 32;
	let mut image = vec![0; width * height];

	for glyph in layout.glyphs() {
		let (_, raster) = font.rasterize(glyph.c, glyph.font_size);

		let x = glyph.x as usize + 16;
		let y = glyph.y as usize + 16;

		for gy in 0..glyph.height {
			for gx in 0..glyph.width {
				image[(y + gy) * width + (x + gx)] = raster[gy * glyph.width + gx];
			}
		}
	}

	Image {
		width,
		height,
		data: image,
	}
}
