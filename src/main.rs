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

#[derive(Debug, clap::Parser)]
struct Args {
	youtube_secret_path: std::path::PathBuf,
}

fn prefixed<'a>(prefix: &'a str, local_name: &'a str) -> xml::name::Name<'a> {
	xml::name::Name::prefixed(local_name, prefix)
}

async fn get_feed_from_channel_id<T: AsRef<str>>(channel_id: T) -> String {
	// TODO: Maybe add some of the following tags:
	// itunes:author
	// itunes:duration
	// itunes:explicit
	
	let channel_id = channel_id.as_ref();
	
	let rss_url = hyper::Uri::builder()
		.scheme("https")
		.authority("www.youtube.com")
		.path_and_query(format!("/feeds/videos.xml?channel_id={channel_id}"))
		.build().unwrap();
	
	let https = hyper_rustls::HttpsConnectorBuilder::new()
		.with_native_roots()
		.https_only()
		.enable_http1()
		.enable_http2()
		.build();
	
	let client = hyper::client::Client::builder().build::<_, hyper::Body>(https);
	let res = client.get(rss_url).await.unwrap();
	
	let body_bytes = hyper::body::to_bytes(res.into_body()).await.unwrap();
	let body_bytes = body_bytes.as_ref();
	
	let writer_inner = Vec::new();
	
	let mut reader = xml::EventReader::new(body_bytes);
	let mut writer = xml::EventWriter::new(writer_inner);
	
	writer.write(
		xml::writer::XmlEvent::start_element("rss")
			.attr(prefixed("xmlns", "dc"     ), "http://purl.org/dc/elements/1.1/"            )
			.attr(prefixed("xmlns", "sy"     ), "http://purl.org/rss/1.0/modules/syndication/")
			.attr(prefixed("xmlns", "admin"  ), "http://webns.net/mvcb/"                      )
			.attr(prefixed("xmlns", "atom"   ), "http://www.w3.org/2005/Atom/"                )
			.attr(prefixed("xmlns", "rdf"    ), "http://www.w3.org/1999/02/22-rdf-syntax-ns#" )
			.attr(prefixed("xmlns", "content"), "http://purl.org/rss/1.0/modules/content/"    )
			.attr(prefixed("xmlns", "itunes" ), "http://www.itunes.com/dtds/podcast-1.0.dtd"  )
			.attr(xml::name::Name::local("version"), "2.0")
	).unwrap();
	
	writer.write(
		xml::writer::XmlEvent::start_element("channel")
	).unwrap();
	
	let mut tree = Vec::new();
	while let Ok(event) = reader.next() {
		match event {
			xml::reader::XmlEvent::EndDocument => {
				break;
			}
			xml::reader::XmlEvent::StartElement { name, attributes, .. } => {
				tree.push(name);
				
				let tree: Vec<_> = tree.iter().map(|name| name.borrow().local_name).collect();
				let tree = tree.as_slice();
				match tree {
					[ "feed" ] => {
						writer.write(
							xml::writer::XmlEvent::start_element("description")
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::characters("dingus description")
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::end_element()
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::start_element("itunes:block")
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::characters("no")
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::end_element()
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::start_element("itunes:image")
								.attr(xml::name::Name::local("href"), "https://yt3.googleusercontent.com/AiO15WuxEOHj2ADa3v9X9euz4MHEzCNZ7XY05JOQUSffoNT4hvWs-dhdZmbkmMlpp5RsnWcorg=s176-c-k-c0x00ffffff-no-rj")
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::end_element()
						).unwrap();
					}
					[ "feed", "entry" ] => {
						writer.write(
							xml::writer::XmlEvent::start_element("item")
						).unwrap();
					}
					[ "feed", "entry", "link" ] => {
						let href = attributes.iter()
							.map(|attribute| attribute.borrow())
							.find(|attribute| attribute.name.local_name == "href")
							.unwrap()
							.value;
						
						let href_uri = hyper::Uri::from_str(href).unwrap();
						let video_id =
							href_uri.query()
								.map(|query| query.split("&")
									.map(|key_value| key_value.split_once("=").unwrap())
									.find(|(key, _)| *key == "v")
									.map(|(_, value)| value)).unwrap().unwrap();
						
						let enclosure_href = format!("http://192.168.1.8:3000/audio/{video_id}.mp3");
						
						writer.write(
							xml::writer::XmlEvent::start_element("enclosure")
								.attr(xml::name::Name::local("url"), enclosure_href.as_str())
								// TODO: maybe try to determine the length
								.attr(xml::name::Name::local("length"), "0")
								.attr(xml::name::Name::local("type"), "audio/mp3")
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::end_element()
						).unwrap();
					}
					[ "feed", "entry", "group", "thumbnail" ] => {
						let url = attributes.iter()
							.map(|attribute| attribute.borrow())
							.find(|attribute| attribute.name.local_name == "url")
							.unwrap()
							.value;
						
						// writer.write(
						// 	xml::writer::XmlEvent::start_element("itunes:image")
						// 		.attr(xml::name::Name::local("href"), url)
						// ).unwrap();
						
						// writer.write(
						// 	xml::writer::XmlEvent::end_element()
						// ).unwrap();
					}
					_ => { }
				}
			}
			xml::reader::XmlEvent::EndElement { .. } => {
				let other_tree: Vec<_> = tree.iter().map(|name| name.borrow().local_name).collect();
				let other_tree = other_tree.as_slice();
				
				match other_tree {
					[ "feed", "entry" ] => {
						writer.write(
							xml::writer::XmlEvent::end_element()
						).unwrap();
					}
					_ => { }
				}
				
				tree.pop();
			}
			xml::reader::XmlEvent::Characters(characters) => {
				let tree: Vec<_> = tree.iter().map(|name| name.borrow().local_name).collect();
				let tree = tree.as_slice();
				match tree {
					[ "feed", "title" ] => {
						writer.write(
							xml::writer::XmlEvent::start_element("title")
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::characters(characters.as_str())
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::end_element()
						).unwrap();
					}
					[ "feed", "entry", "title" ] => {
						writer.write(
							xml::writer::XmlEvent::start_element("title")
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::characters(characters.as_str())
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::end_element()
						).unwrap();
					}
					[ "feed", "entry", "group", "description" ] => {
						writer.write(
							xml::writer::XmlEvent::start_element("description")
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::characters(characters.as_str())
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::end_element()
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::start_element("itunes:subtitle")
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::characters(characters.as_str())
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::end_element()
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::start_element("itunes:summary")
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::characters(characters.as_str())
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::end_element()
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::start_element("content:encoded")
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::cdata("dingus content")
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::end_element()
						).unwrap();
					}
					[ "feed", "entry", "id" ] => {
						writer.write(
							xml::writer::XmlEvent::start_element("guid")
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::characters(characters.as_str())
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::end_element()
						).unwrap();
					}
					[ "feed", "entry", "published" ] => {
						writer.write(
							xml::writer::XmlEvent::start_element("pubDate")
						).unwrap();
						
						let published = chrono::DateTime::parse_from_rfc3339(characters.as_str()).unwrap();
						let published = published.to_rfc2822();
						writer.write(
							xml::writer::XmlEvent::characters(published.as_str())
						).unwrap();
						
						writer.write(
							xml::writer::XmlEvent::end_element()
						).unwrap();
					}
					_ => { }
				}
			}
			_ => { }
		}
	}
	
	// channel
	writer.write(
		xml::writer::XmlEvent::end_element()
	).unwrap();
	
	// rss
	writer.write(
		xml::writer::XmlEvent::end_element()
	).unwrap();
	
	let idk = writer.into_inner();
	let kdi = std::str::from_utf8(idk.as_slice()).unwrap();
	
	String::from(kdi)
}

use std::{collections::{HashMap, BTreeMap}, str::FromStr, ffi::OsStr, any, io::Read};

use clap::Parser;
use rusqlite::OptionalExtension;


mod audio {
	pub enum State {
		NotDownloaded,
		Downloading,
		Downloaded,
	}
	impl rusqlite::types::FromSql for State {
		fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
			let value = value.as_i64()?;
			match value {
				0 => Ok(State::NotDownloaded),
				1 => Ok(State::Downloading),
				2 => Ok(State::Downloaded),
				_ => Err(rusqlite::types::FromSqlError::InvalidType),
			}
		}
	}
	impl rusqlite::ToSql for State {
		fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
			match self {
				State::NotDownloaded => Ok(0.into()),
				State::Downloading => Ok(1.into()),
				State::Downloaded => Ok(2.into()),
			}
		}
	}
}

struct App {
	youtube_hub: tokio::sync::Mutex<google_youtube3::YouTube<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>>,
	connection: tokio::sync::Mutex<rusqlite::Connection>,
	audio_thing: tokio::sync::Mutex<HashMap<String, std::sync::Arc<tokio::sync::Notify>>>,
}

impl App {
	async fn new(youtube_secret: google_youtube3::oauth2::ServiceAccountKey) -> Self {
		let connection = tokio::sync::Mutex::new(
			rusqlite::Connection::open("data.db").unwrap());
		let audio_thing = tokio::sync::Mutex::new(
			HashMap::new());
		
		connection.lock().await.execute_batch("
			CREATE TABLE IF NOT EXISTS Channel
				( id TEXT PRIMARY KEY
				
				, name TEXT
				, description TEXT
				, image_url TEXT
				
				, last_updated_timestamp INTEGER
			);
			
			CREATE TABLE IF NOT EXISTS Video
				( id TEXT PRIMARY KEY
				, channel_id TEXT 
				
				, name TEXT
				, description TEXT
				, image_url TEXT
				, published_timestamp INTEGER
				
				, FOREIGN KEY(channel_id) REFERENCES Channel(id)
			);
			
			CREATE TABLE IF NOT EXISTS Audio
				( video_id TEXT PRIMARY KEY
				
				, accessed_timestamp INTEGER
			);
		").unwrap();
		
		let auth = google_youtube3::oauth2::ServiceAccountAuthenticator::builder(
			youtube_secret
		).build().await.unwrap();
		let youtube_hub = tokio::sync::Mutex::new(
			google_youtube3::YouTube::new(hyper::Client::builder().build(hyper_rustls::HttpsConnectorBuilder::new().with_native_roots().https_or_http().enable_http1().enable_http2().build()), auth));
		
		Self {
			youtube_hub,
			connection,
			audio_thing,
		}
	}
	
	async fn update_channel_data<T: AsRef<str>>(&self, channel_id: T) {
		let channel_id = channel_id.as_ref();
		
		let mut connection = self.connection.lock().await;
		let youtube_hub = self.youtube_hub.lock().await;
		
		let channel_in_db = {
			connection.query_row(
				"SELECT EXISTS (SELECT 1 FROM Channel WHERE id = ?)",
				rusqlite::params![channel_id],
				|row| row.get::<_, bool>(0),
			).unwrap()
		};
		
		if channel_in_db {
			// Download and save video information, as long as
			// we don't hit a primary key violation (which means
			// that we're all updated).
			
			let mut page_token: Option<String> = None;
			let transaction = connection.transaction().unwrap();
			
			'downloading: loop {
				let (_, results) = {
					let search = {
						youtube_hub.search()
							.list(&vec!["id".to_string(), "snippet".to_string()])
							.add_type("video")
							.channel_id(channel_id)
							.order("date")
							.max_results(50)
					};
					
					let search = if let Some(page_token) = page_token {
						search.page_token(page_token.as_str())
					} else {
						search
					};
					
					search.doit().await.unwrap()
				};
				
				page_token = results.next_page_token;
				
				let items = results.items.unwrap();
				for item in items.iter() {
					let snippet = item.snippet.as_ref().unwrap();
					dbg!(&snippet);
					let result = transaction.execute(
						"INSERT INTO Video (id, channel_id, name, description, image_url, published_timestamp) VALUES (?, ?, ?, ?, ?, ?)",
						rusqlite::params![
							item.id.as_ref().unwrap().video_id.as_ref().unwrap(),
							snippet.channel_id.as_ref().unwrap(),
							snippet.title.as_ref().unwrap(),
							snippet.description.as_ref().unwrap(),
							snippet
								.thumbnails.as_ref().unwrap()
								.default.as_ref().unwrap()
								.url.as_ref().unwrap(),
							snippet.published_at.as_ref().unwrap(),
							chrono::Utc::now(),
							audio::State::NotDownloaded,
						],
					);
					
					if let Err(rusqlite::Error::SqliteFailure(rusqlite::ffi::Error { code: rusqlite::ffi::ErrorCode::ConstraintViolation, extended_code: rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY }, _)) = result {
						break 'downloading;
					}
					
					result.unwrap();
				}
				
				if page_token.is_none() {
					break;
				}
			}
			
			transaction.commit().unwrap();
		} else {
			// Download all video information from scratch.
			
			let (_, results) = youtube_hub.search()
				.list(&vec!["id".to_string(), "snippet".to_string()])
				.add_type("channel")
				.channel_id(channel_id)
				.doit().await.unwrap();
			
			let items = results.items.unwrap();
			assert!(items.len() == 1);
			
			let transaction = connection.transaction().unwrap();
			
			{
				let channel = &items[0];
				let snippet = channel.snippet.as_ref().unwrap();
				
				transaction.execute(
					"INSERT INTO Channel (id, name, description, image_url) VALUES (?, ?, ?, ?)",
					rusqlite::params![
						channel_id,
						snippet.channel_title,
						snippet.description,
						snippet
							.thumbnails.as_ref().unwrap()
							.default.as_ref().unwrap()
							.url.as_ref().unwrap(),
					],
				).unwrap();
			}
			
			let mut page_token: Option<String> = None;
			
			loop {
				let (_, results) = {
					let search = {
						youtube_hub.search()
							.list(&vec!["id".to_string(), "snippet".to_string()])
							.add_type("video")
							.channel_id(channel_id)
							.order("date")
							.max_results(50)
					};
					
					let search = if let Some(page_token) = page_token {
						search.page_token(page_token.as_str())
					} else {
						search
					};
					
					search.doit().await.unwrap()
				};
				
				page_token = results.next_page_token;
				
				let items = results.items.unwrap();
				for item in items.iter() {
					let snippet = item.snippet.as_ref().unwrap();
					transaction.execute(
						"INSERT INTO Video (id, channel_id, name, description, image_url, published_timestamp) VALUES (?, ?, ?, ?, ?, ?)",
						rusqlite::params![
							item.id.as_ref().unwrap().video_id.as_ref().unwrap(),
							snippet.channel_id.as_ref().unwrap(),
							snippet.title.as_ref().unwrap(),
							snippet.description.as_ref().unwrap(),
							snippet
								.thumbnails.as_ref().unwrap()
								.default.as_ref().unwrap()
								.url.as_ref().unwrap(),
							snippet.published_at.as_ref().unwrap(),
							chrono::Utc::now(),
							audio::State::NotDownloaded,
						],
					).unwrap();
				}
				
				if page_token.is_none() {
					break;
				}
			}
			
			transaction.commit().unwrap();
		}
	}
	
	async fn get_video_audio<T: AsRef<str>>(&self, video_id: T) -> std::fs::File {
		let video_id = video_id.as_ref();
		
		let audio_filename = format!("{video_id}.mp3");
		
		let audio_thing = self.audio_thing.lock().await;
		let connection = self.connection.lock().await;
		
		let row = {
			connection.query_row(
				"SELECT state FROM Audio WHERE video_id = ?",
				rusqlite::params![video_id],
				|row| row.get::<_, audio::State>(0),
			).optional().unwrap()
		};
		
		async fn download<'a, T: AsRef<str>>(
			app: &App,
			video_id: T,
			audio_filename: T,
			connection: tokio::sync::MutexGuard<'a, rusqlite::Connection>,
			mut audio_thing: tokio::sync::MutexGuard<'a, HashMap<String, std::sync::Arc<tokio::sync::Notify>>>,
		) {
			// Download audio and register it into the db.
			
			let video_id = video_id.as_ref();
			
			connection.execute(
				"INSERT INTO Audio (video_id, accessed_timestamp) VALUES (?, ?)",
				rusqlite::params![video_id, chrono::Utc::now()],
			).unwrap();
			std::mem::drop(connection);
			
			let notify_downloaded = std::sync::Arc::new(tokio::sync::Notify::new());
			audio_thing.insert(String::from(video_id), notify_downloaded.clone());
			std::mem::drop(audio_thing);
			
			// MAYBE: Log stdout and stderr.
			async_process::Command::new("yt-dlp")
				.stdout(async_process::Stdio::null())
				.stderr(async_process::Stdio::null())
				.arg(format!("https://www.youtube.com/watch?v={video_id}"))
				.arg("--extract-audio")
				.arg("--audio-format")
				.arg("mp3")
				.arg("-o")
				.arg(audio_filename.as_ref())
				.spawn()
				.unwrap()
				.output().await.unwrap();
			
			{
				let connection = app.connection.lock().await;
				connection.execute(
					"UPDATE Audio SET state = ? WHERE video_id = ?",
					rusqlite::params![audio::State::Downloaded, video_id],
				).unwrap();
			}
			
			notify_downloaded.notify_waiters();
		}
		
		// try select audio from table
		// - if it exists:
		//   - if there's no notify, that means it's downloaded
		//   - if there's a notify, wait on the notify
		// - if it doesn't exist, you have to download it: start downloading, add notify
		
		if let Some(state) = row {
			match state {
				audio::State::NotDownloaded => {
					todo!()
				}
				audio::State::Downloaded => {
					// Do nothing.
				}
				audio::State::Downloading => {
					let notify_downloaded = {
						audio_thing.get(video_id)
							.map(|notify_downloaded| notify_downloaded.clone())
					};
					
					if let Some(notify_downloaded) = notify_downloaded {
						// Wait until the audio has been downloaded.
						
						std::mem::drop(audio_thing);
						notify_downloaded.notified().await;
					} else {
						// This means that it attempted to download the file before, but failed.
						
						connection.execute(
							"DELETE FROM Audio WHERE video_id = ?",
							rusqlite::params![video_id]
						).unwrap();
						
						download(
							self,
							video_id,
							&audio_filename,
							connection,
							audio_thing,
						).await;
					}
				}
			}
		} else {
			download(
				self,
				video_id,
				&audio_filename,
				connection,
				audio_thing,
			).await;
		};
		
		std::fs::File::open(audio_filename).unwrap()
	}
}

async fn dingus(
	app: std::sync::Arc<App>,
	request: hyper::Request<hyper::Body>,
) -> anyhow::Result<hyper::Response<hyper::Body>> {
	let path = std::path::Path::new(request.uri().path());
	if let &hyper::Method::GET = request.method() {
		if let Some(parent) = path.parent().map(|parent| parent.as_os_str()).and_then(|parent| parent.to_str()) {
			match parent {
				"/audio" => {
					assert!(matches!(path.extension().and_then(|extension| extension.to_str()), Some("mp3")));
					let video_id = path.file_stem().unwrap().to_str().unwrap();
					let mut audio_file = app.get_video_audio(video_id).await;
					let mut audio = Vec::new();
					audio_file.read_to_end(&mut audio).unwrap();
					anyhow::Ok(
						hyper::Response::builder()
							.header(hyper::header::CONTENT_TYPE, "audio/mp3")
							.body(hyper::Body::from(audio))
							.unwrap()
					)
				}
				"/feed/channel_id" => {
					let channel_id = path.file_name().unwrap().to_str().unwrap();
					let feed = get_feed_from_channel_id(&channel_id).await;
					anyhow::Ok(
						hyper::Response::builder()
							.header(hyper::header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")
							.body(hyper::Body::from(feed))
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
							.body(hyper::Body::from(Vec::new()))
							.unwrap())
				}
			}
		} else {
			Ok(
				hyper::Response::builder()
					.status(hyper::StatusCode::NOT_FOUND)
					.body(hyper::Body::from(Vec::new()))
					.unwrap())
		}
	} else {
		Ok(
			hyper::Response::builder()
				.status(hyper::StatusCode::NOT_FOUND)
				.body(hyper::Body::from(Vec::new()))
				.unwrap())
	}
}

#[tokio::main]
async fn main() {
	let args = Args::parse();
	
	let youtube_secret = google_youtube3::oauth2::read_service_account_key(args.youtube_secret_path).await.unwrap();
	
	// NOTE: It should theoretically be possible to have an immutable
	// reference instead of an Arc, but I couldn't figure it out.
	
	let app = std::sync::Arc::new(App::new(youtube_secret).await);
	
	{
		// app.test("UC3cpN6gcJQqcCM6mxRUo_dA").await;
		app.update_channel_data("UC3cpN6gcJQqcCM6mxRUo_dA").await;
	}
	
	let make_service = {
		let app = app.clone();
		hyper::service::make_service_fn(move |_conn: &hyper::server::conn::AddrStream| {
			let app = app.clone();
			async move {
				anyhow::Ok(
					hyper::service::service_fn(move |request| {
						let app = app.clone();
						println!("got a request!");
						dingus(app, request)
					})
				)
			}
		})
	};
	
	// let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
	let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 3000));
	let server = hyper::Server::bind(&addr).serve(make_service);
	let result = server.await;
	dbg!(result);
}
