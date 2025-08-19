use std::{net::SocketAddr, sync::Arc};

use axum::{
	body::{Bytes, Full},
	extract::Path,
	http::StatusCode,
	response::{IntoResponse, Response},
	routing::get,
	Extension, Router,
};
use common::{FontStyle, FontVariant, FontWeight};
use fontprovider::CachedFont;
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::fontprovider::FontProvider;

mod fontprovider;

#[tokio::main]
async fn main() {
	tracing_subscriber::fmt::init();

	let provider = FontProvider::new("fonts");

	let app = Router::new()
		.route("/font/:family/:style/:weight", get(fonts))
		.route("/ping", get(ping))
		.layer(Extension(Arc::new(RwLock::new(provider))));

	let addr = SocketAddr::from(([0, 0, 0, 0], 2561));
	tracing::debug!("listening on {addr}");
	axum::Server::bind(&addr)
		.serve(app.into_make_service())
		.await
		.unwrap()
}

#[derive(Debug, Deserialize)]
struct Font {
	family: String,
	style: FontStyle,
	weight: FontWeight,
}

async fn fonts(
	provider: Extension<Arc<RwLock<FontProvider>>>,
	Path(Font {
		family,
		style,
		weight,
	}): Path<Font>,
) -> Response {
	tracing::info!("request for {family} {style} {weight}");

	let font = {
		let res = {
			provider
				.read()
				.await
				.variant_cached(&family, FontVariant { style, weight })
		};

		match res {
			CachedFont::Available { font } => font,
			CachedFont::Known => {
				let mut lock = provider.write().await;
				match lock.variant(&family, FontVariant { style, weight }) {
					Some(font) => font,
					None => return (StatusCode::NOT_FOUND, "not found").into_response(),
				}
			}
			CachedFont::Unknown => {
				tracing::info!("font unknown");
				return (StatusCode::NOT_FOUND, "not found").into_response();
			}
		}
	};

	Response::builder()
		.header("content-type", "application/octet-stream")
		.status(200)
		.body(Full::new(Bytes::from(font)))
		.unwrap()
		.into_response()
}

async fn ping() -> Response {
	tracing::debug!("pinged!");

	(StatusCode::OK, "pong!").into_response()
}
