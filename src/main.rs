use std::{collections::{HashMap, BTreeMap}, str::FromStr, ffi::{OsStr, OsString}, any, io::Read};

use clap::Parser;
use lazy_static::lazy_static;
use rusqlite::OptionalExtension;

#[derive(Debug, clap::Parser)]
struct Args {
	#[arg(long)]
	youtube_secret_path: std::path::PathBuf,
	#[arg(long)]
	working_dir: std::path::PathBuf,
}

lazy_static! {
	static ref AUDIO_CACHE_PATH: &'static std::path::Path = std::path::Path::new("audio_cache");
	static ref AUDIO_TMP_PATH: &'static std::path::Path = std::path::Path::new("audio_tmp");
}

struct AppConfig {
	domain: String,
	audio_cache_size: u64,
	youtube_update_delay: chrono::Duration,
}

fn prefixed<'a>(prefix: &'a str, local_name: &'a str) -> xml::name::Name<'a> {
	xml::name::Name::prefixed(local_name, prefix)
}

trait EventWriterExt {
	fn nest<'a, F>(
		&mut self,
		start_element: xml::writer::events::StartElementBuilder,
		add_elements: F,
	) -> xml::writer::Result<()>
	where
		F: FnOnce(&mut Self) -> xml::writer::Result<()>
	;
}

impl<W: std::io::Write> EventWriterExt for xml::writer::EventWriter<W> {
	fn nest<'a, F>(
		&mut self,
		start_element: xml::writer::events::StartElementBuilder,
		add_elements: F,
	) -> xml::writer::Result<()>
	where
		F: FnOnce(&mut Self) -> xml::writer::Result<()>
	{
		self.write(start_element)?;
		add_elements(self)?;
		self.write(xml::writer::XmlEvent::end_element())?;
		Ok(())
	}
}

struct App {
	config: AppConfig,
	
	youtube_hub: google_youtube3::YouTube<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
	connection: tokio::sync::Mutex<rusqlite::Connection>,
	audio_thing: tokio::sync::Mutex<HashMap<String, std::sync::Arc<tokio::sync::Notify>>>,
}

impl App {
	async fn new(config: AppConfig, youtube_secret: google_youtube3::oauth2::ServiceAccountKey) -> Self {
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
				
				, last_accessed_timestamp INTEGER
			);
		").unwrap();
		
		let auth = google_youtube3::oauth2::ServiceAccountAuthenticator::builder(
			youtube_secret
		).build().await.unwrap();
		let youtube_hub =
			google_youtube3::YouTube::new(hyper::Client::builder().build(hyper_rustls::HttpsConnectorBuilder::new().with_native_roots().https_or_http().enable_http1().enable_http2().build()), auth);
		
		Self {
			config,
			
			youtube_hub,
			connection,
			audio_thing,
		}
	}
	
	async fn get_feed_from_channel_id<T: AsRef<str>>(&self, channel_id: T) -> String {
		// TODO: Maybe add some of the following tags:
		// itunes:author
		// itunes:duration
		// itunes:explicit
		
		let channel_id = channel_id.as_ref();
		
		self.update_channel_data(channel_id).await;
		
		let connection = self.connection.lock().await;
		
		struct Channel {
			name: String,
			description: String,
			image_url: String,
		}
		
		let channel = {
			connection.query_row(
				"SELECT name, description, image_url FROM Channel WHERE id = ?",
				rusqlite::params![channel_id],
				|row| Ok(
					Channel {
						name: row.get(0)?,
						description: row.get(1)?,
						image_url: row.get(2)?}),
			).unwrap()
		};
		
		struct Video {
			id: String,
			name: String,
			description: String,
			image_url: String,
			published_at: chrono::DateTime<chrono::Utc>,
		}
		
		let videos: Vec<_> = {
			let mut statement = connection.prepare(
				"SELECT id, name, description, image_url, published_timestamp FROM Video WHERE channel_id = ? ORDER BY published_timestamp"
			).unwrap();
			
			let rows = statement.query_map(
				rusqlite::params![channel_id],
				|row| Ok(
					Video {
						id: row.get(0)?,
						name: row.get(1)?,
						description: row.get(2)?,
						image_url: row.get(3)?,
						published_at: row.get(4)?}),
			).unwrap();
			
			rows.collect()
		};
		
		let writer_inner = Vec::new();
		let mut writer = xml::EventWriter::new(writer_inner);
		
		writer.nest(
			xml::writer::XmlEvent::start_element("rss")
				.attr(prefixed("xmlns", "dc"     ), "http://purl.org/dc/elements/1.1/"            )
				.attr(prefixed("xmlns", "sy"     ), "http://purl.org/rss/1.0/modules/syndication/")
				.attr(prefixed("xmlns", "admin"  ), "http://webns.net/mvcb/"                      )
				.attr(prefixed("xmlns", "atom"   ), "http://www.w3.org/2005/Atom/"                )
				.attr(prefixed("xmlns", "rdf"    ), "http://www.w3.org/1999/02/22-rdf-syntax-ns#" )
				.attr(prefixed("xmlns", "content"), "http://purl.org/rss/1.0/modules/content/"    )
				.attr(prefixed("xmlns", "itunes" ), "http://www.itunes.com/dtds/podcast-1.0.dtd"  )
				.attr(xml::name::Name::local("version"), "2.0"),
			|writer| writer.nest(
				xml::writer::XmlEvent::start_element("channel"),
				|writer| {
					// Metadata
					writer.nest(
						xml::writer::XmlEvent::start_element("title"),
						|writer| writer.write(
							xml::writer::XmlEvent::characters(&channel.name)
						)
					)?;
					writer.nest(
						xml::writer::XmlEvent::start_element("description"),
						|writer| writer.write(
							xml::writer::XmlEvent::characters(&channel.description)
						)
					)?;
					writer.nest(
						xml::writer::XmlEvent::start_element("block"),
						|writer| writer.write(
							xml::writer::XmlEvent::characters("no")
						)
					)?;
					writer.nest(
						xml::writer::XmlEvent::start_element("itunes:image")
							.attr(xml::name::Name::local("href"), &channel.image_url),
						|_| Ok(())
					)?;
					
					// Items
					for video in videos {
						let video = video.unwrap();
						
						writer.nest(
							xml::writer::XmlEvent::start_element("item"),
							|writer| {
								let video_id = video.id;
								
								writer.nest(
									xml::writer::XmlEvent::start_element("title"),
									|writer| writer.write(
										xml::writer::XmlEvent::characters(&video.name)
									)
								)?;
								
								writer.nest(
									xml::writer::XmlEvent::start_element("description"),
									|writer| writer.write(
										xml::writer::XmlEvent::characters(&video.description)
									)
								)?;
								
								writer.nest(
									xml::writer::XmlEvent::start_element("itunes:subtitle"),
									|writer| writer.write(
										xml::writer::XmlEvent::characters(&video.description)
									)
								)?;
								
								writer.nest(
									xml::writer::XmlEvent::start_element("itunes:summary"),
									|writer| writer.write(
										xml::writer::XmlEvent::characters(&video.description)
									)
								)?;
								
								// TODO: escape cdata
								writer.nest(
									xml::writer::XmlEvent::start_element("content:encoded"),
									|writer| writer.write(
										xml::writer::XmlEvent::cdata(&video.description)
									)
								)?;
								
								writer.nest(
									xml::writer::XmlEvent::start_element("guid"),
									|writer| writer.write(
										xml::writer::XmlEvent::characters(&video_id)
									)
								)?;
								
								let domain = &self.config.domain;
								let enclosure_href = format!("http://{domain}:3000/audio/{video_id}.mp3");
								
								writer.nest(
									xml::writer::XmlEvent::start_element("enclosure")
										.attr(xml::name::Name::local("url"), enclosure_href.as_str())
										.attr(xml::name::Name::local("length"), "0")
										.attr(xml::name::Name::local("type"), "audio/mp3"),
									|_| Ok(())
								)?;
								
								let published = video.published_at.to_rfc2822();
								
								writer.nest(
									xml::writer::XmlEvent::start_element("pubDate"),
									|writer| writer.write(
										xml::writer::XmlEvent::characters(published.as_str())
									)
								)?;
								
								writer.nest(
									xml::writer::XmlEvent::start_element("itunes:image")
										.attr(xml::name::Name::local("href"), &video.image_url),
									|_| Ok(())
								)?;
								
								Ok(())
							}
						)?;
					}
					
					Ok(())
				}
			)
		).unwrap();
		
		let idk = writer.into_inner();
		let kdi = std::str::from_utf8(idk.as_slice()).unwrap();
		
		String::from(kdi)
	}
	
	fn update_channel_data_sync<T: AsRef<str>>(
		handle: tokio::runtime::Handle,
		connection: &mut rusqlite::Connection,
		youtube_hub: &google_youtube3::YouTube<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
		youtube_update_delay: chrono::Duration,
		channel_id: T,
	) {
		let channel_id = channel_id.as_ref();
		
		let last_updated_timestamp: Option<chrono::DateTime<chrono::Utc>> = {
			connection.query_row(
				"SELECT last_updated_timestamp FROM Channel WHERE id = ?",
				rusqlite::params![channel_id],
				|row| row.get(0),
			).optional().unwrap()
		};
		
		let update_video_data = {
			if let Some(last_updated_timestamp) = last_updated_timestamp {
				chrono::Utc::now() - last_updated_timestamp > youtube_update_delay
			} else {
				true
			}
		};
		
		if update_video_data {
			// Download and save video information, until we hit
			// a primary key violation (which means that we're
			// all updated).
			
			let transaction = connection.transaction().unwrap();
			
			// Get channel data.
			let search = {
				 youtube_hub.search()
					.list(&vec!["id".to_string(), "snippet".to_string()])
					.add_type("channel")
					.channel_id(channel_id)
			};
			
			let (_, results) = tokio_scoped::scoped(&handle).scope(|scope| {
				scope.block_on(async {
					search.doit().await
				})
			}).unwrap();
			
			let items = results.items.unwrap();
			assert!(items.len() == 1);
			
			let channel = &items[0];
			let snippet = channel.snippet.as_ref().unwrap();
			
			transaction.execute(
				"INSERT OR REPLACE INTO Channel (id, name, description, image_url, last_updated_timestamp) VALUES (?, ?, ?, ?, ?)",
				rusqlite::params![
					channel_id,
					snippet.channel_title,
					snippet.description,
					snippet
						.thumbnails.as_ref().unwrap()
						.default.as_ref().unwrap()
						.url.as_ref().unwrap(),
					chrono::Utc::now(),
				],
			).unwrap();
			
			// Get videos data.
			let mut page_token: Option<String> = None;
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
					
					tokio_scoped::scoped(&handle).scope(|scope| {
						scope.block_on(async {
							search.doit().await
						})
					}).unwrap()
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
		}
	}
	
	async fn update_channel_data<T: AsRef<str>>(&self, channel_id: T) {
		App::update_channel_data_sync(
			tokio::runtime::Handle::current(),
			&mut *self.connection.lock().await,
			&self.youtube_hub,
			self.config.youtube_update_delay,
			channel_id,
		);
	}
	
	async fn get_video_audio<T: AsRef<str>>(&self, video_id: T) -> std::fs::File {
		let video_id = video_id.as_ref();
		
		let audio_filename = format!("{video_id}.mp3");
		
		let audio_downloaded = {
			self.connection.lock().await.query_row(
				"SELECT EXISTS (SELECT 1 FROM Audio WHERE video_id = ?)",
				rusqlite::params![video_id],
				|row| row.get::<_, bool>(0),
			).unwrap()
		};
		
		let mut update_last_accessed_timestamp = true;
		
		if !audio_downloaded {
			let audio_thing = self.audio_thing.lock().await;
			let notify_downloaded = audio_thing.get(video_id);
			if let Some(notify_downloaded) = notify_downloaded {
				notify_downloaded.notified().await;
			} else {
				// Download audio and register it into the db.
				
				let notify_downloaded = {
					let mut audio_thing = audio_thing;
					let notify_downloaded = std::sync::Arc::new(tokio::sync::Notify::new());
					audio_thing.insert(String::from(video_id), notify_downloaded.clone());
					notify_downloaded
				};
				
				// TODO: Delete the tmp_path if the command fails.
				// TODO: Log stdout and stderr.
				
				// Run command to download audio.
				let tmp_path = std::path::Path::new(".")
					.join(AUDIO_TMP_PATH.as_os_str())
					.join(&video_id);
				
				async_process::Command::new("yt-dlp")
					.stdout(async_process::Stdio::null())
					.stderr(async_process::Stdio::null())
					.arg(format!("https://www.youtube.com/watch?v={video_id}"))
					.arg("--extract-audio")
					.arg("--audio-format")
						.arg("mp3")
					.arg("--paths")
						.arg(&tmp_path)
					.arg("-o")
						.arg(&audio_filename)
					.spawn()
					.unwrap()
					.output().await.unwrap();
				
				// Move audio filename from temporary folder
				// to cache.
				let audio_old_path = tmp_path.join(&audio_filename);
				
				let audio_new_path = std::path::Path::new(".")
					.join(AUDIO_CACHE_PATH.as_os_str())
					.join(&audio_filename);
				
				std::fs::rename(audio_old_path, audio_new_path).unwrap();
				
				// Clean up temporary directory.
				std::fs::remove_dir_all(tmp_path).unwrap();
				
				// calculate size of all downloaded videos
				// if the size is above the max
				// get audio sorted ascending by last access
				// while the size is above the max
				// delete audio (remove from table, etc.)
				// if the audio to-be-deleted happens to be the just downloaded audio, then stop
				
				update_last_accessed_timestamp = false;
				self.connection.lock().await.execute(
					"INSERT INTO Audio (video_id, last_accessed_timestamp) VALUES (?, ?)",
					rusqlite::params![video_id, chrono::Utc::now()],
				).unwrap();
				
				self.audio_thing.lock().await.remove(video_id).unwrap();
				
				notify_downloaded.notify_waiters();
			}
		}
		
		if update_last_accessed_timestamp {
			self.connection.lock().await.execute(
				"UPDATE Audio SET last_accessed_timestamp = ? WHERE video_id = ?",
				rusqlite::params![chrono::Utc::now(), video_id],
			).unwrap();
		}
		
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
					let feed = app.get_feed_from_channel_id(&channel_id).await;
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
	
	std::env::set_current_dir(args.working_dir).unwrap();
	
	let app_config = AppConfig {
		audio_cache_size: 1024 * 1024 * 16,
		domain: String::from("172.30.111.80"),
		youtube_update_delay: chrono::Duration::minutes(15),
	};
	
	let app = std::sync::Arc::new(App::new(app_config, youtube_secret).await);
	
	{
		// app.test("UC3cpN6gcJQqcCM6mxRUo_dA").await;
		// app.update_channel_data("UC3cpN6gcJQqcCM6mxRUo_dA").await;
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
