use clap::Parser;

#[derive(Debug, clap::Parser)]
struct Args {
	#[arg(long)]
	youtube_secret_path: std::path::PathBuf,
	#[arg(long)]
	working_dir: std::path::PathBuf,
}

#[derive(Clone)]
pub struct AppConfig {
	domain: String,
	audio_cache_size: u64,
	youtube_update_delay: chrono::Duration,
}

mod web {
    use std::io::Read;

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

	fn prefixed<'a>(prefix: &'a str, local_name: &'a str) -> xml::name::Name<'a> {
		xml::name::Name::prefixed(local_name, prefix)
	}

	async fn get_feed_from_channel_id<T: AsRef<str>>(
		app_config: crate::AppConfig,
		youtube_data_request_sender: tokio::sync::mpsc::UnboundedSender<(tokio::sync::oneshot::Sender<crate::youtube_data::Channel>, crate::youtube_data::ChannelIdentifier)>,
		channel_id: T,
	) -> String {
		// TODO: Maybe add some of the following tags:
		// itunes:author
		// itunes:duration
		// itunes:explicit
		
		let channel_id = channel_id.as_ref();
		
		let (response_sender, response_receiver) = tokio::sync::oneshot::channel();
		youtube_data_request_sender.send((
			response_sender,
			crate::youtube_data::ChannelIdentifier::Id(String::from(channel_id)),
		));
		let channel = response_receiver.await.unwrap();
		
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
					for video in channel.videos {
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
								
								let domain = &app_config.domain;
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

	pub async fn server(
		config: crate::AppConfig,
		audio_request_sender: tokio::sync::mpsc::UnboundedSender<(tokio::sync::oneshot::Sender<std::fs::File>, String)>,
		youtube_data_request_sender: tokio::sync::mpsc::UnboundedSender<(tokio::sync::oneshot::Sender<crate::youtube_data::Channel>, crate::youtube_data::ChannelIdentifier)>,
		request: hyper::Request<hyper::Body>,
	) -> anyhow::Result<hyper::Response<hyper::Body>> {
		let path = std::path::Path::new(request.uri().path());
		if let &hyper::Method::GET = request.method() {
			if let Some(parent) = path.parent().map(|parent| parent.as_os_str()).and_then(|parent| parent.to_str()) {
				match parent {
					"/audio" => {
						assert!(matches!(path.extension().and_then(|extension| extension.to_str()), Some("mp3")));
						let video_id = path.file_stem().unwrap().to_str().unwrap();

						let (response_sender, response_receiver) = tokio::sync::oneshot::channel();
						audio_request_sender.send((
							response_sender,
							String::from(video_id),
						)).unwrap();
		
						let mut audio_file = response_receiver.await.unwrap();

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
						let feed = get_feed_from_channel_id(
							config.clone(),
							youtube_data_request_sender,
							&channel_id,
						).await;
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
}

mod youtube_data {
    use rusqlite::OptionalExtension;

	#[derive(Debug)]
	pub struct Channel {
		pub name: String,
		pub description: String,
		pub image_url: String,
		pub videos: Vec<Video>,
	}
	
	#[derive(Debug)]
	pub struct Video {
		pub id: String,
		pub name: String,
		pub description: String,
		pub image_url: String,
		pub published_at: chrono::DateTime<chrono::Utc>,
	}
	
	#[derive(Debug)]
	pub enum ChannelIdentifier {
		Id(String),
		Name(String),
	}
	
	pub async fn server(
		app_config: crate::AppConfig,
		youtube_secret: google_youtube3::oauth2::ServiceAccountKey,
		mut request_receiver: tokio::sync::mpsc::UnboundedReceiver<(tokio::sync::oneshot::Sender<Channel>, ChannelIdentifier)>
	) {
		let connection = std::sync::Arc::new(tokio::sync::Mutex::new(
			rusqlite::Connection::open("youtube.db").unwrap()));
		
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
		").unwrap();
		
		let auth = google_youtube3::oauth2::ServiceAccountAuthenticator::builder(
			youtube_secret
		).build().await.unwrap();
		let youtube_hub =
			google_youtube3::YouTube::new(hyper::Client::builder().build(hyper_rustls::HttpsConnectorBuilder::new().with_native_roots().https_or_http().enable_http1().enable_http2().build()), auth);
		
		while let Some((sender, channel_identifier)) = request_receiver.recv().await {
			let connection = connection.clone();
			let youtube_hub = youtube_hub.clone();
			tokio::spawn(async move {
				let channel_id = match channel_identifier {
					ChannelIdentifier::Id(channel_id) => channel_id,
					ChannelIdentifier::Name(_) => todo!(),
				};
				
				let mut connection = connection.lock().await;
				
				update_channel_data_sync(
					tokio::runtime::Handle::current(),
					&mut connection,
					&youtube_hub,
					app_config.youtube_update_delay,
					&channel_id,
				);

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
			
					rows.map(|row| row.unwrap()).collect()
				};

				let channel = {
					connection.query_row(
						"SELECT name, description, image_url FROM Channel WHERE id = ?",
						rusqlite::params![channel_id],
						|row| Ok(
							Channel {
								name: row.get(0)?,
								description: row.get(1)?,
								image_url: row.get(2)?,
								videos}),
					).unwrap()
				};
		
				sender.send(channel).unwrap();
			});
		}
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
			let request = {
				 youtube_hub.channels()
					.list(&vec!["id".to_string(), "snippet".to_string(), "contentDetails".to_string()])
					.add_id(channel_id)
			};
			
			let (_, results) = tokio_scoped::scoped(&handle).scope(|scope| {
				scope.block_on(async {
					request.doit().await
				})
			}).unwrap();
			
			let items = results.items.unwrap();
			assert!(items.len() == 1);
			
			let channel = &items[0];
			let snippet = channel.snippet.as_ref().unwrap();
			let content_details = channel.content_details.as_ref().unwrap();

			let playlist_id = {
				content_details
					.related_playlists.as_ref().unwrap()
					.uploads.as_ref().unwrap()
			};
			
			transaction.execute(
				"INSERT OR REPLACE INTO Channel (id, name, description, image_url, last_updated_timestamp) VALUES (?, ?, ?, ?, ?)",
				rusqlite::params![
					channel_id,
					snippet.title,
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
					let request = {
						youtube_hub.playlist_items()
							.list(&vec!["id".to_string(), "snippet".to_string(), "contentDetails".to_string()])
							.playlist_id(&playlist_id)
							.max_results(50)
					};
					
					let request = if let Some(page_token) = page_token {
						request.page_token(page_token.as_str())
					} else {
						request
					};
					
					tokio_scoped::scoped(&handle).scope(|scope| {
						scope.block_on(async {
							request.doit().await
						})
					}).unwrap()
				};
				
				dbg!(&results.next_page_token);
				dbg!(results.items.as_ref().unwrap().len());
				page_token = results.next_page_token;
				
				let items = results.items.unwrap();
				for item in items.iter() {
					let snippet = item.snippet.as_ref().unwrap();
					let result = transaction.execute(
						"INSERT INTO Video (id, channel_id, name, description, image_url, published_timestamp) VALUES (?, ?, ?, ?, ?, ?)",
						rusqlite::params![
							snippet.resource_id.as_ref().unwrap().video_id.as_ref().unwrap(),
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
}

mod audio_download {
    use std::collections::HashMap;

	lazy_static::lazy_static! {
		pub static ref AUDIO_CACHE_PATH: &'static std::path::Path = std::path::Path::new("audio_cache");
		pub static ref AUDIO_TMP_PATH: &'static std::path::Path = std::path::Path::new("audio_tmp");
		pub static ref AUDIO_OUTPUT_PATH: &'static std::path::Path = std::path::Path::new("audio_output");
	}
	
	pub async fn server(
		app_config: crate::AppConfig,
		mut audio_request_receiver: tokio::sync::mpsc::UnboundedReceiver<(tokio::sync::oneshot::Sender<std::fs::File>, String)>
	) {
	
		while let Some((sender, video_id)) = audio_request_receiver.recv().await {
			let audio_thing = audio_thing.clone();
			let connection = connection.clone();
			tokio::spawn(async move {
				let audio_filename = format!("{video_id}.mp3");
				let audio_path = std::path::Path::new(".")
					.join(AUDIO_CACHE_PATH.as_os_str())
					.join(&audio_filename);
			
				if !audio_downloaded {
					let audio_thing_lock = audio_thing.lock().await;
				
					let notify_downloaded = audio_thing_lock.get(&video_id).map(|notify_downloaded| notify_downloaded.clone());
				
					let notify_downloaded = {
						if let Some(notify_downloaded) = notify_downloaded {
							std::mem::drop(audio_thing_lock);
							notify_downloaded
						} else {
							let mut audio_thing_lock = audio_thing_lock;
							let notify_downloaded = {
								let notify_downloaded = std::sync::Arc::new(tokio::sync::Notify::new());
								audio_thing_lock.insert(video_id.clone(), notify_downloaded.clone());
								notify_downloaded
							};
							std::mem::drop(audio_thing_lock);
							let notify_downloaded_clone = notify_downloaded.clone();
						
							let audio_path = audio_path.clone();
							let audio_thing = audio_thing.clone();
							tokio::spawn(async move {
								// Download audio and register it into the db.
							
								// TODO: Delete the tmp_path if the command fails.
								// TODO: Log stdout and stderr.
							
								// Run command to download audio.
								let tmp_path = std::path::Path::new(".")
									.join(AUDIO_TMP_PATH.as_os_str())
									.join(&video_id);
							
								dbg!("starting command");
								let output = {
									tokio::process::Command::new("yt-dlp")
										.stdin(std::process::Stdio::null())
										.arg(format!("https://www.youtube.com/watch?v={video_id}"))
										.arg("--external-downloader")
											.arg("aria2c")
										.arg("--external-downloader-args")
											.arg("-j 4 -x 8 -s 8 -k 1M")
										.arg("--extract-audio")
										.arg("--audio-format")
											.arg("mp3")
										.arg("--paths")
											.arg(&tmp_path)
										.arg("-o")
											.arg(&audio_filename)
										.output().await.unwrap()
								};
							
								dbg!(String::from_utf8(output.stderr).unwrap());
								dbg!(String::from_utf8(output.stdout).unwrap());
							
								dbg!("what");
							
								// NOTE: There could theoretically be
								// an issue, if the audio gets added
								// to the db, and the program stops
								// afterward for some reason, without
								// actually moving the file.
							
								// Not gonna deal with that for now though.
							
								// Add audio to database.
								let audio_tmp_path = tmp_path.join(&audio_filename);
								let metadata = std::fs::metadata(&audio_tmp_path).unwrap();
							
								connection.lock().await.execute(
									"INSERT INTO Audio (video_id, size, last_accessed_timestamp) VALUES (?, ?, ?)",
									rusqlite::params![video_id, metadata.len(), chrono::Utc::now()],
								).unwrap();
							
								// Move audio from temporary folder
								// to cache.
								std::fs::rename(&audio_tmp_path, &audio_path).unwrap();
							
								// Clean up temporary directory.
								std::fs::remove_dir_all(tmp_path).unwrap();
							
								// Make sure audio cache size doesn't exceed limit.
								#[derive(Debug)]
								struct Audio {
									video_id: String,
									size: u64,
								}
							
								let mut audios: Vec<_> = {
									let connection = connection.lock().await;
									let mut statement = connection.prepare(
										"SELECT video_id, size FROM Audio ORDER BY last_accessed_timestamp"
									).unwrap();
	
									let rows = statement.query_map(
										rusqlite::params![],
										|row| Ok(
											Audio {
												video_id: row.get(0)?,
												size: row.get(1)?}),
									).unwrap();
	
									let rows = rows.map(|row| row.unwrap()).collect();
									std::mem::drop(statement);
									std::mem::drop(connection);
									rows
								};
								let mut audio_cache_size: u64 = 0;
							
								dbg!(&audios);
							
								let audio_cache_path = std::path::Path::new(".")
									.join(AUDIO_CACHE_PATH.as_os_str());
								if audio_cache_size > app_config.audio_cache_size {
									while !audios.is_empty()
										&& audio_cache_size > app_config.audio_cache_size
									{
										let audio = audios.pop().unwrap();
										// Make sure we don't
										// delete the audio we
										// just downloaded.
										if audio.video_id != video_id {
											let audio_path = audio_cache_path.join(format!("{}.mp3", audio.video_id));
											audio_cache_size -= audio.size;
											std::fs::remove_file(&audio_path).unwrap();
										}
									}
								}
							
								audio_thing.lock().await.remove(&video_id).unwrap();
								notify_downloaded.notify_waiters();
							});
						
							notify_downloaded_clone
						}
					};
				
					notify_downloaded.notified().await;
				}
				// TODO: Do something when the send is not successful.
				sender.send(std::fs::File::open(audio_path).unwrap()).unwrap();
			});
		}
	}
}

mod audio_db {
	struct AudioSomething {
		drop_sender: tokio::sync::mpsc::UnboundedSender<String>,
		file: std::fs::File,
		video_id: String,
	}
	
	impl Drop for AudioSomething {
		fn drop(&mut self) {
			self.drop_sender.send(self.video_id);
		}
	}
	
	impl AudioSomething {
		fn file_mut(&mut self) -> &mut std::fs::File { &mut self.file }
	}
	
	pub async fn server(
		app_config: crate::AppConfig,
		mut audio_request_receiver: tokio::sync::mpsc::UnboundedReceiver<(tokio::sync::oneshot::Sender<std::fs::File>, String)>,
		mut youtube_download_request_sender: tokio::sync::mpsc::UnboundedSender<(tokio::sync::oneshot::Sender<std::fs::File>, String)>,
	) {
		let audio_idk: std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<tokio::sync::Notify>>>> = std::sync::Arc::new(tokio::sync::Mutex::new(HashMap::new()));
	
		let connection: std::sync::Arc<tokio::sync::Mutex<rusqlite::Connection>> = std::sync::Arc::new(tokio::sync::Mutex::new(rusqlite::Connection::open("audio.db").unwrap()));
		connection.lock().await.execute_batch("
			CREATE TABLE IF NOT EXISTS Audio
				( video_id TEXT PRIMARY KEY
			
				, size INTEGER
			
				, last_accessed_timestamp INTEGER
			);
		").unwrap();

		let (drop_sender, mut drop_receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
		let mut audio_thingie: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
		
		loop {
			tokio::select! {
				// NOTE: Make sure to always serve all audio requests, before handling
				// the drops, so that we don't delete an audio file which was actually
				// requested.
			
				biased;

				request = audio_request_receiver.recv() => {
					let audio_downloaded = {
						connection.lock().await.query_row(
							"SELECT EXISTS (SELECT 1 FROM Audio WHERE video_id = ?)",
							rusqlite::params![video_id],
							|row| row.get::<_, bool>(0),
						).unwrap()
					};

					// check if it exists
					// if it doesn't, send a request to the downloader
					// probably add 1 to its use count
				}

				video_id = drop_receiver.recv() => {
					let video_id = video_id.unwrap();
					let references = audio_thingie.get_mut(&video_id).unwrap();
					*references -= 1;
					if *references == 0 {
						audio_thingie.remove(&video_id);
					}
				}
			}
			
			// run code that tries to clean up stuff
		}
		
		// give out references to files
		// keep track of how many references are currently out for any given file
		// when a file has just been added, or a file has gone from having references, to having no references
		// try to clean up the files (if you have to)
		
		// dictionary: video_id -> mutex with the count
	}
}

#[tokio::main]
async fn main() {
	// console_subscriber::init();
	
	let args = Args::parse();
	
	let youtube_secret = google_youtube3::oauth2::read_service_account_key(args.youtube_secret_path).await.unwrap();
	
	std::env::set_current_dir(args.working_dir).unwrap();
	std::fs::create_dir_all(std::path::Path::new(".").join(audio_download::AUDIO_TMP_PATH.as_os_str())).unwrap();
	std::fs::create_dir_all(std::path::Path::new(".").join(audio_download::AUDIO_CACHE_PATH.as_os_str())).unwrap();
	
	let app_config = AppConfig {
		audio_cache_size: 1024 * 1024 * 100,
		domain: String::from("192.168.1.139"),
		youtube_update_delay: chrono::Duration::minutes(15),
	};
	let app_config2 = app_config.clone();
	let app_config3 = app_config.clone();
	let app_config4 = app_config.clone();
	
	let (audio_request_sender, audio_request_receiver) = tokio::sync::mpsc::unbounded_channel();
	let (youtube_data_request_sender, youtube_data_request_receiver) = tokio::sync::mpsc::unbounded_channel();
	
	// NOTE: It should theoretically be possible to have an immutable
	// reference instead of an Arc, but I couldn't figure it out.
	
	{
		// app.test("UC3cpN6gcJQqcCM6mxRUo_dA").await;
		// app.update_channel_data("UC3cpN6gcJQqcCM6mxRUo_dA").await;
	}
	
	let make_service = {
		let audio_request_sender = audio_request_sender.clone();
		let youtube_data_request_sender = youtube_data_request_sender.clone();
		let app_config4 = app_config4.clone();
		hyper::service::make_service_fn(move |_conn: &hyper::server::conn::AddrStream| {
			let audio_request_sender = audio_request_sender.clone();
			let youtube_data_request_sender = youtube_data_request_sender.clone();
			let app_config4 = app_config4.clone();
			async move {
				anyhow::Ok(
					hyper::service::service_fn(move |request| {
						let audio_request_sender = audio_request_sender.clone();
						let youtube_data_request_sender = youtube_data_request_sender.clone();
						let app_config4 = app_config4.clone();
						println!("got a request!");
						web::server(
							app_config4,
							audio_request_sender,
							youtube_data_request_sender,
							request,
						)
					})
				)
			}
		})
	};
	
	// let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
	tokio::join! {
		async {
			let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 3000));
			let server = hyper::Server::bind(&addr).serve(make_service);
			let result = server.await;
			dbg!(result);
		},
		audio_download::server(
			app_config2,
			audio_request_receiver,
		),
		youtube_data::server(
			app_config3,
			youtube_secret,
			youtube_data_request_receiver,
		),
	};
}
