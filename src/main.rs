// async fn get_channel_id_from_username<T: AsRef<str>>(username: T) {
// 	let secret: google_youtube3::oauth2::ApplicationSecret = google_youtube3::oauth2::
	
// 	let mut hub = google_youtube3::YouTube::new(
// 		hyper::Client::builder()
// 			.build(
// 				google_youtube3::hyper_rustls::HttpsConnectorBuilder::new()
// 					.with_native_roots()
// 					.https_or_http()
// 					.enable_http1()
// 					.enable_http2()
// 					.build()), auth);
// }

async fn get_feed_from_channel_id<T: AsRef<str>>(channel_id: T) -> atom_syndication::Feed {
	let channel_id = channel_id.as_ref();
	
	let rss_url = hyper::Uri::builder()
		.scheme("https")
		.authority("www.youtube.com")
		.path_and_query(format!("/feeds/videos.xml?channel_id={channel_id}"))
		.build().unwrap();
	
	let https = hyper_tls::HttpsConnector::new();
	
	let client = hyper::client::Client::builder().build::<_, hyper::Body>(https);
	let res = client.get(rss_url).await.unwrap();
	
	let body_bytes = hyper::body::to_bytes(res.into_body()).await.unwrap();
	let feed = atom_syndication::Feed::read_from(body_bytes.as_ref()).unwrap();
	
	feed
}

use std::{collections::{HashMap, BTreeMap}, str::FromStr, ffi::OsStr, any};

use hyper::http::request;
use rusqlite::OptionalExtension;
use tokio::net::unix::SocketAddr;


mod audio {
	pub enum State {
		Downloading,
		Downloaded,
	}
	impl rusqlite::types::FromSql for State {
		fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
			let value = value.as_i64()?;
			match value {
				0 => Ok(State::Downloading),
				1 => Ok(State::Downloaded),
				_ => Err(rusqlite::types::FromSqlError::InvalidType),
			}
		}
	}
	impl rusqlite::ToSql for State {
		fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
			match self {
				State::Downloading => Ok(0.into()),
				State::Downloaded => Ok(1.into()),
			}
		}
	}
}

struct App {
	connection: tokio::sync::Mutex<rusqlite::Connection>,
	
	audio_thing: tokio::sync::Mutex<HashMap<String, std::sync::Arc<tokio::sync::Notify>>>,
}

impl App {
	async fn new() -> Self {
		let connection = tokio::sync::Mutex::new(
			rusqlite::Connection::open("data.db").unwrap());
		let audio_thing = tokio::sync::Mutex::new(
			HashMap::new());
		
		connection.lock().await.execute_batch("
			CREATE TABLE IF NOT EXISTS Audio
				( video_id TEXT PRIMARY KEY
				, accessed_timestamp INTEGER
				, state INTEGER);
		").unwrap();
		
		Self {
			connection,
			audio_thing,
		}
	}
	
	async fn get_video_audio<T: AsRef<str>>(&self, video_id: T) -> std::fs::File {
		let video_id = video_id.as_ref();
		
		let audio_path = format!("{video_id}.mp3");
		
		let row = {
			let connection = self.connection.lock().await;
			connection.query_row(
				"SELECT state FROM Audio WHERE video_id = ?",
				rusqlite::params![video_id],
				|row| row.get::<_, audio::State>(0),
			).optional().unwrap()
		};
		
		if let Some(state) = row {
			match state {
				audio::State::Downloaded => {
					// Do nothing.
				}
				audio::State::Downloading => {
					// Wait until the audio has been downloaded.
					
					let notify_downloaded = {
						let audio_thing = self.audio_thing.lock().await;
						audio_thing.get(video_id)
							.map(|notify_downloaded| notify_downloaded.clone())
					};
					
					if let Some(notify_downloaded) = notify_downloaded {
						notify_downloaded.notified().await;
					}
				}
			}
		} else {
			// Download audio and register it into the db.
			
			let notify_downloaded = std::sync::Arc::new(tokio::sync::Notify::new());
			{
				let mut audio_thing = self.audio_thing.lock().await;
				audio_thing.insert(String::from(video_id), notify_downloaded.clone());
			}
			
			{
				let connection = self.connection.lock().await;
				connection.execute(
					"INSERT INTO Audio (video_id, accessed_timestamp, state) VALUES (?, ?, ?)",
					rusqlite::params![video_id, chrono::Utc::now(), audio::State::Downloading],
				).unwrap();
			}
			
			async_process::Command::new("yt-dlp")
				.arg(format!("https://www.youtube.com/watch?v={video_id}"))
				.arg("--extract-audio")
				.arg("--audio-format")
				.arg("mp3")
				.arg("-o")
				.arg(&audio_path)
				.spawn()
				.unwrap()
				.output().await.unwrap();
			
			{
				let connection = self.connection.lock().await;
				connection.execute(
					"UPDATE Audio SET state = ? WHERE video_id = ?",
					rusqlite::params![audio::State::Downloaded, video_id],
				).unwrap();
			}
			
			notify_downloaded.notify_waiters();
			
			// NOTE TO SELF:
			// not sure if there's a better way to think of this
			// this is sorta good enough, but it feels like I'm missing something
		}
		
		std::fs::File::open(audio_path).unwrap()
	}
}

fn convert_feed(feed: &mut atom_syndication::Feed) {
	let new_entries: Vec<_> = feed.entries()
		.iter()
		.map(|entry| {
			let mut new_entry = entry.clone();
			new_entry.set_extensions(atom_syndication::extension::ExtensionMap::default());
			let new_links: Vec<_> = entry.links()
				.iter()
				.map(|link| {
					let mut new_link = link.clone();
					let href = new_link.href();
					let href_uri = hyper::Uri::from_str(href).unwrap();
					let video_id =
						href_uri.query()
							.map(|query| query.split("&")
								.map(|key_value| key_value.split_once("=").unwrap())
								.find(|(key, _)| *key == "v")
								.map(|(_, value)| value)).unwrap().unwrap();
					new_link.set_href(format!("192.168.0.126/audio/{video_id}.mp3"));
					new_link.set_rel("enclosure");
					new_link
				})
				.collect();
			new_entry.set_links(new_links);
			let extension = atom_syndication::extension::ExtensionBuilder::default()
				.name("description".to_string())
				.value(Some("TODO".to_string()))
				.build();
			new_entry
		})
		.collect();
	
	feed.set_entries(new_entries);
	feed.set_extensions(atom_syndication::extension::ExtensionMap::default());
	feed.set_links(Vec::new());
}

async fn dingus(request: hyper::Request<hyper::Body>) -> anyhow::Result<hyper::Response<String>> {
	let path = std::path::Path::new(request.uri().path());
	if let &hyper::Method::GET = request.method() {
		if let Some(parent) = path.parent().map(|parent| parent.as_os_str()).and_then(|parent| parent.to_str()) {
			match parent {
				"/audio" => {
					assert!(matches!(path.extension().and_then(|extension| extension.to_str()), Some("mp3")));
					let video_id = path.file_stem().unwrap();
					todo!()
				}
				"/feed/channel_id" => {
					let channel_id = path.file_name().unwrap().to_str().unwrap();
					let mut feed = get_feed_from_channel_id(&channel_id).await;
					convert_feed(&mut feed);
					let feed_string = feed.to_string();
					anyhow::Ok(
						hyper::Response::builder()
							.header(hyper::header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")
							.body(feed_string)
							.unwrap()
					)
				}
				"/feed/channel_username" => {
					todo!()
				}
				_ => {
					Ok(
						hyper::Response::builder()
							.status(hyper::StatusCode::NOT_FOUND)
							.body(String::from(""))
							.unwrap())
				}
			}
		} else {
			Ok(
				hyper::Response::builder()
					.status(hyper::StatusCode::NOT_FOUND)
					.body(String::from(""))
					.unwrap())
		}
	} else {
		Ok(
			hyper::Response::builder()
				.status(hyper::StatusCode::NOT_FOUND)
				.body(String::from(""))
				.unwrap())
	}
}

#[tokio::main]
async fn main() {
	// {
	// 	let idk = "https://www.relay.fm/cortex/feed";
	// 	let rss_url = hyper::Uri::from_str(idk).unwrap();
		
	// 	let https = hyper_tls::HttpsConnector::new();
		
	// 	let client = hyper::client::Client::builder().build::<_, hyper::Body>(https);
	// 	let res = client.get(rss_url).await.unwrap();
		
	// 	let body_bytes = hyper::body::to_bytes(res.into_body()).await.unwrap();
	// 	let feed = atom_syndication::Feed::read_from(body_bytes.as_ref()).unwrap();
		
	// 	dbg!(feed);
	// }
	
	{
		let mut feed = get_feed_from_channel_id("UC3cpN6gcJQqcCM6mxRUo_dA").await;
		dbg!(&feed);
		convert_feed(&mut feed);
		dbg!(&feed);
		dbg!(feed.to_string());
	}
	
	let make_service = hyper::service::make_service_fn(move |_conn| async {
		anyhow::Ok(
			hyper::service::service_fn(move |request| {
				dingus(request)
			})
		)
	});
	
	// let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
	let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 3000));
	let server = hyper::Server::bind(&addr).serve(make_service);
	
	let result = server.await;
	dbg!(result);
	
	// let app = App::new().await;
	
	// let mut audio = app.get_video_audio("Cct-6xrC-fQ").await;
	// let mut a = [0; 128];
	// audio.read_exact(&mut a).unwrap();
	// dbg!(a);
}
