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
	convert::{Infallible, TryInto},
	future::Future,
	net::{SocketAddr, TcpListener, TcpStream},
	pin::Pin,
	str::FromStr,
	task::{Context, Poll},
	time::Instant,
};

use bempline::Document;
use chrono::Utc;
use crateimage::png::PngEncoder;
use fontprovider::FontProvider;
use hyper::{body::HttpBody, service::Service, Body, Request, Response, Server};
use mavourings::query::Query;
use serde::Serialize;
use std::sync::Arc;
use text::{Operation, Text};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::statistics::Statistics;

struct Textual {
	config: Config,
	statistics: RwLock<Statistics>,
	font_provider: RwLock<FontProvider>,
}

struct MakeSvc {
	textual: Arc<Textual>,
}

impl<T> Service<T> for MakeSvc {
	type Response = Svc;
	type Error = &'static str;
	type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

	fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
		Poll::Ready(Ok(()))
	}

	fn call(&mut self, _: T) -> Self::Future {
		let textual = self.textual.clone();
		let fut = async move { Ok(Svc { textual }) };
		Box::pin(fut)
	}
}

struct Svc {
	textual: Arc<Textual>,
}

impl Service<Request<Body>> for Svc {
	type Response = Response<Body>;
	type Error = &'static str;
	type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

	fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
		Poll::Ready(Ok(()))
	}

	fn call(&mut self, req: Request<Body>) -> Self::Future {
		let tex = self.textual.clone();
		Box::pin(async { Ok(Self::task(req, tex).await) })
	}
}

impl Svc {
	async fn task(req: Request<Body>, textual: Arc<Textual>) -> Response<Body> {
		let response = match Self::serve(req, textual.clone()).await {
			Ok(resp) => resp,
			Err(e) => Response::builder()
				.header("content-type", "text/plain")
				.body(Body::from(e.to_string()))
				.unwrap(),
		};

		match response.headers().get("content-type").map(|hv| hv.to_str()) {
			Some(Ok(mime)) => {
				let mut stats = textual.statistics.write().await;
				//TODO: gen- check exact or log upper instead?
				stats.add(mime, response.body().size_hint().lower() as usize);
			}
			_ => {
				//TODO: Maybe print here that the content-type was unset or could not be parsed as a string
			}
		}

		response
	}

	async fn serve(
		req: Request<Body>,
		textual: Arc<Textual>,
	) -> Result<Response<Body>, Infallible> {
		let mut query_str = match req.uri().query() {
			None => return Ok(Self::serve_tool().await),
			Some(s) if s.is_empty() => return Ok(Self::serve_tool().await),
			Some(s) => s.to_owned(),
		};
		let query: Query = query_str.parse().unwrap();

		if query.has_bool("info") && !query.has_bool("forceraw") {
			let stats = textual.statistics.read().await;
			let provider = textual.font_provider.read().await;

			let text = format!(
				"{}\n\nimage sent: {}\nhtml sent: {}\ntotal requests: {}\n\nfonts in cache: {}",
				Utc::now().format("%H:%M UTC\n%a %B %-d %Y"),
				bytes_to_human(stats.image()),
				bytes_to_human(stats.html()),
				stats.requests(),
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

		let agent = req.headers().get("user-agent").unwrap().to_str().unwrap();

		let clientaddr = req
			.headers()
			.get("X-Forwarded-For")
			.map(|h| h.to_str().unwrap_or("unknown"))
			.unwrap_or("unknown");

		println!(
			"connection: {}\n\tua: {}\n\tpath: {}",
			clientaddr,
			agent,
			req.uri().path_and_query().unwrap().to_string()
		);

		if query.has_bool("me") && !query.has_bool("forceraw") && !query.has_bool("info") {
			let referrer = match req.headers().get(hyper::header::REFERER) {
				None => "unknown",
				Some(hv) => match hv.to_str() {
					Ok(hstring) => hstring,
					Err(e) => "unknown",
				},
			};

			let text = format!(
				"IP: {}\n\nUser Agent\n{}\n\nReferrer\n{}",
				clientaddr, agent, referrer
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

		let text: Operation = query.into();

		// Find the hostname we should use for the image link in the opengraph tags
		let host = textual
			.config
			.meta_host()
			.or(req
				.headers()
				.get("host")
				.map(|hv| hv.to_str().ok())
				.flatten())
			.unwrap_or("localhost");

		let scheme = textual
			.config
			.scheme()
			.or(req.uri().scheme_str())
			.unwrap_or(if host == "localhost" { "http" } else { "https" });

		if text.forceraw {
			// Image
			Ok(make_image(textual, text).await?)
		} else {
			let link = format!("{}://{}?{}&forceraw", scheme, host, query_str);
			Ok(make_meta(textual, text, link).await?)
		}
	}

	async fn serve_tool() -> Response<Body> {
		mavourings::file_string_reply("tool.html").await.unwrap()
	}
}

#[tokio::main]
async fn main() {
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

	let address = SocketAddr::new(config.listen(), config.port());
	let textual = Textual {
		config,
		font_provider: RwLock::new(provider),
		statistics: RwLock::new(Statistics::default()),
	};

	Server::bind(&address)
		.serve(MakeSvc {
			textual: Arc::new(textual),
		})
		.await
		.unwrap();
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

async fn make_image(textual: Arc<Textual>, op: Operation) -> Result<Response<Body>, Infallible> {
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
		.body(Body::from(encoded_buffer))
		.map_err(|e| panic!())
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
) -> Result<Response<Body>, Infallible> {
	let mut t = Document::from_str(TEMPLATE).unwrap();

	t.set("text", op.full_text());
	t.set("alt", op.get_alt());
	t.set("twitter_image", format!("{}&aspect=1.8", link));
	t.set("og_image", format!("{}&aspect=1.8", link));
	t.set("image", link);
	t.set("font", String::new());
	t.set("hex_color", String::new());

	let render = t.compile();

	Response::builder()
		.header("content-type", "text/html")
		.header("content-length", render.len())
		.body(Body::from(render))
		.map_err(|e| panic!())
}
