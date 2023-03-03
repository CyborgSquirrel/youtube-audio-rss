use std::sync::Arc;

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
    use std::{io::Read, sync::Arc};

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

	async fn get_feed(
		app_config: Arc<crate::AppConfig>,
		youtube_data_request_sender: tokio::sync::mpsc::UnboundedSender<(tokio::sync::oneshot::Sender<crate::youtube_data::Channel>, crate::youtube_data::ChannelIdentifier)>,
		channel_identifier: &crate::youtube_data::ChannelIdentifier,
	) -> String {
		// TODO: Maybe add some of the following tags:
		// itunes:author
		// itunes:duration
		// itunes:explicit
		
		let (response_sender, response_receiver) = tokio::sync::oneshot::channel();
		youtube_data_request_sender.send((
			response_sender,
			channel_identifier.clone(),
		)).unwrap();
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
							xml::writer::XmlEvent::characters(&channel.username)
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
								
								writer.nest(
									xml::writer::XmlEvent::start_element("content:encoded"),
									|writer| writer.write(
										xml::writer::XmlEvent::cdata(
											&xml::escape::escape_str_pcdata(&video.description))
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
		
		let bytes = writer.into_inner();
		let string = std::str::from_utf8(bytes.as_slice()).unwrap();
		
		String::from(string)
	}	

	pub async fn server(
		config: Arc<crate::AppConfig>,
		audio_cache_request_sender: tokio::sync::mpsc::UnboundedSender<(tokio::sync::oneshot::Sender<crate::audio_cache::CachedAudioFile>, String)>,
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
						audio_cache_request_sender.send((
							response_sender,
							String::from(video_id),
						)).unwrap();
		
						let mut cached_audio_file = response_receiver.await.unwrap();

						let mut audio = Vec::new();
						cached_audio_file.file_mut().read_to_end(&mut audio).unwrap();
						anyhow::Ok(
							hyper::Response::builder()
								.header(hyper::header::CONTENT_TYPE, "audio/mp3")
								.body(hyper::Body::from(audio))
								.unwrap()
						)
					}
					"/feed/channel_id" => {
						let channel_id = path
							.file_name().unwrap()
							.to_str().unwrap()
							.to_string();
						let feed = get_feed(
							config.clone(),
							youtube_data_request_sender,
							&crate::youtube_data::ChannelIdentifier::Id(channel_id),
						).await;
						anyhow::Ok(
							hyper::Response::builder()
								.header(hyper::header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")
								.body(hyper::Body::from(feed))
								.unwrap()
						)
					}
					"/feed/channel_custom_url" => {
						let channel_custom_url = path
							.file_name().unwrap()
							.to_str().unwrap()
							.to_string();
						let feed = get_feed(
							config.clone(),
							youtube_data_request_sender,
							&crate::youtube_data::ChannelIdentifier::CustomUrl(channel_custom_url),
						).await;
						anyhow::Ok(
							hyper::Response::builder()
								.header(hyper::header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")
								.body(hyper::Body::from(feed))
								.unwrap()
						)
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
    use std::sync::Arc;

    use rusqlite::OptionalExtension;
    use tokio::sync::Mutex;

	#[derive(Debug)]
	pub struct Channel {
		pub username: String,
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
	
	#[derive(Debug, Clone)]
	pub enum ChannelIdentifier {
		Id(String),
		CustomUrl(String),
	}
	
	pub async fn server(
		app_config: Arc<crate::AppConfig>,
		youtube_secret: google_youtube3::oauth2::ServiceAccountKey,
		mut request_receiver: tokio::sync::mpsc::UnboundedReceiver<(tokio::sync::oneshot::Sender<Channel>, ChannelIdentifier)>
	) {
		// Set up database.
		let connection = rusqlite::Connection::open("youtube.db").unwrap();
		connection.execute_batch("
			CREATE TABLE IF NOT EXISTS Channel
				( id TEXT PRIMARY KEY
				
				, username TEXT
				, description TEXT
				, image_url TEXT
				, custom_url
				
				, last_updated_timestamp INTEGER

				, UNIQUE(username)
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

		let connection = Arc::new(Mutex::new(connection));
		
		// Initialize youtube api.
		let auth = {
			google_youtube3::oauth2::ServiceAccountAuthenticator::builder(
				youtube_secret
			).build().await.unwrap()
		};
		let youtube_hub = {
			google_youtube3::YouTube::new(
				hyper::Client::builder()
					.build(
						hyper_rustls::HttpsConnectorBuilder::new()
							.with_native_roots()
							.https_or_http()
							.enable_http1()
							.enable_http2()
							.build()), auth)
		};
		
		while let Some((sender, channel_identifier)) = request_receiver.recv().await {
			let connection = connection.clone();
			let youtube_hub = youtube_hub.clone();
			let app_config = app_config.clone();
			tokio::spawn(async move {
				let mut connection = connection.lock().await;
				
				let channel_id = {
					update_channel_data(
						app_config.clone(),
						&mut connection,
						&youtube_hub,
						&channel_identifier,
					).await.unwrap()
				};
				
				struct ChannelMetadata {
					username: String,
					description: String,
					image_url: String,
				}

				let channel_metadata = {
					connection.query_row(
						"SELECT username, description, image_url FROM Channel WHERE id = ?",
						rusqlite::params![channel_id],
						|row| Ok(
							ChannelMetadata {
								username: row.get(0)?,
								description: row.get(1)?,
								image_url: row.get(2)?})).unwrap()
				};

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
				
				let channel = Channel {
					username: channel_metadata.username,
					description: channel_metadata.description,
					image_url: channel_metadata.image_url,
					videos,
				};

				sender.send(channel).unwrap();
			});
		}
	}
	
	#[derive(Debug)]
	pub enum Error {
		InvalidChannelId,
		InvalidChannelCustomUrl,
	}
	
	async fn update_channel_data(
		app_config: Arc<crate::AppConfig>,
		connection: &mut rusqlite::Connection,
		youtube_hub: &google_youtube3::YouTube<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
		channel_identifier: &ChannelIdentifier,
	) -> Result<String, Error> {
		let channel_id = {
			match channel_identifier {
				ChannelIdentifier::Id(id) => id.clone(),
				ChannelIdentifier::CustomUrl(custom_url) => {
					let channel_id: Option<String> = {
						connection.query_row(
							"SELECT id FROM Channel WHERE custom_url = ?",
							rusqlite::params![custom_url],
							|row| row.get(0),
						).optional().unwrap()
					};
					
					if let Some(channel_id) = channel_id {
						channel_id
					} else {
						// Scrape the channel_id from the page, cause' there's no way to get a
						// channel_id from a channel's custom_url using the youtube api :(.
						
						let https = hyper_rustls::HttpsConnectorBuilder::new()
							.with_native_roots()
							.https_only()
							.enable_http1()
							.build();
					
						let client: hyper::Client<_, hyper::Body> = hyper::Client::builder().build(https);
					
						let uri = hyper::Uri::builder()
							.scheme("https")
							.authority("www.youtube.com")
							.path_and_query(format!("/{}", &custom_url))
							.build()
							.unwrap();
						
						let response = client.get(uri).await.unwrap();
						let (_head, body) = response.into_parts();
						let bytes = hyper::body::to_bytes(body).await.unwrap();
						let text = std::str::from_utf8(&bytes).unwrap();
						
						// TODO: check for this not working
						
						let regex_source = r#"<link rel="alternate" type="application/rss[+]xml" title="RSS" href="https://www[.]youtube[.]com/feeds/videos[.]xml[?]channel_id=([^"]*)">"#;
						let regex = regex::RegexBuilder::new(regex_source).build().unwrap();
						let channel_id = regex.captures(text).unwrap()
							.get(1).unwrap()
							.as_str().to_string();
					
						channel_id
					}
				}
			}
		};
		
		update_channel_data_sync(
			tokio::runtime::Handle::current(),
			connection,
			&youtube_hub,
			app_config.youtube_update_delay,
			&channel_id,
		)
			.map(|_| channel_id)
			.map_err(|err| match err {
				Error::InvalidChannelId => {
					if let ChannelIdentifier::CustomUrl(_) = channel_identifier {
						Error::InvalidChannelCustomUrl
					} else {
						err
					}
				}
				_ => err,
			})
	}
	
	fn update_channel_data_sync<T: AsRef<str>>(
		handle: tokio::runtime::Handle,
		connection: &mut rusqlite::Connection,
		youtube_hub: &google_youtube3::YouTube<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
		youtube_update_delay: chrono::Duration,
		channel_id: T,
	) -> Result<(), Error> {
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
			// Download and save video information, until we hit a primary key violation
			// (which means that we're all updated).
			
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
			
			let Some(items) = results.items else {
				return Err(Error::InvalidChannelId);
			};

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
				"INSERT OR REPLACE INTO Channel (id, username, description, image_url, last_updated_timestamp) VALUES (?, ?, ?, ?, ?)",
				rusqlite::params![
					channel.id.as_ref().unwrap(),
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
		
		Ok(())
	}
}

mod audio_download {
    use std::{path::PathBuf, sync::Arc, collections::HashSet};

    use tokio::sync::{mpsc, oneshot, Mutex};

	lazy_static::lazy_static! {
		static ref AUDIO_TMP_PATH: &'static std::path::Path = std::path::Path::new("audio_tmp");
		static ref AUDIO_OUTPUT_PATH: &'static std::path::Path = std::path::Path::new("audio_output");
	}
	
	#[derive(Debug)]
	pub enum Error {
		AlreadyDownloading,
		YtDlpError(String),
	}
	
	pub async fn server(
		app_config: Arc<crate::AppConfig>,
		mut request_receiver: mpsc::UnboundedReceiver<(oneshot::Sender<Result<PathBuf, Error>>, String)>,
	) {
		std::fs::remove_dir_all(std::path::Path::new(".").join(AUDIO_TMP_PATH.as_os_str())).unwrap();
		std::fs::create_dir_all(std::path::Path::new(".").join(AUDIO_TMP_PATH.as_os_str())).unwrap();

		std::fs::remove_dir_all(std::path::Path::new(".").join(AUDIO_OUTPUT_PATH.as_os_str())).unwrap();
		std::fs::create_dir_all(std::path::Path::new(".").join(AUDIO_OUTPUT_PATH.as_os_str())).unwrap();

		let currently_downloading: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

		while let Some((sender, video_id)) = request_receiver.recv().await {
			let mut currently_downloading_lock = currently_downloading.lock().await;
			if currently_downloading_lock.contains(&video_id) {
				sender.send(Err(Error::AlreadyDownloading)).unwrap();
			} else {
				currently_downloading_lock.insert(video_id.clone());
				std::mem::drop(currently_downloading_lock);

				let currently_downloading = currently_downloading.clone();
				tokio::spawn(async move {
					let audio_filename = format!("{video_id}.mp3");

					let audio_output_path = std::path::Path::new(".")
						.join(AUDIO_OUTPUT_PATH.as_os_str())
						.join(&audio_filename);
			
					// TODO: Delete the tmp_path if the command fails.
					// TODO: Log stdout and stderr.
			
					let tmp_path = std::path::Path::new(".")
						.join(AUDIO_TMP_PATH.as_os_str())
						.join(&video_id);
			
					let audio_tmp_path = tmp_path
						.join(&audio_filename);
					
					// Run command to download audio.
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
			
					let success = output.status.success();

					if success {
						// Move audio from temporary directory to output.
						std::fs::rename(&audio_tmp_path, &audio_output_path).unwrap();
					}
			
					// Clean up temporary directory.
					std::fs::remove_dir_all(tmp_path).unwrap();
			
					if success {
						sender.send(Ok(audio_output_path)).unwrap();
					} else {
						sender.send(Err(Error::YtDlpError(String::from_utf8(output.stderr).unwrap()))).unwrap();
					}
					currently_downloading.lock().await.remove(&video_id);
				});
			}
		}
	}
}

mod audio_cache {
    use std::{sync::Arc, collections::HashMap, path::PathBuf};

    use tokio::sync::{Mutex, Notify, mpsc, oneshot};

	lazy_static::lazy_static! {
		static ref AUDIO_CACHE_PATH: &'static std::path::Path = std::path::Path::new("./audio_cache");
	}

	#[derive(Debug)]
	pub struct CachedAudioFile {
		drop_sender: tokio::sync::mpsc::UnboundedSender<String>,
		file: std::fs::File,
		video_id: String,
	}
	
	impl Drop for CachedAudioFile {
		fn drop(&mut self) {
			self.drop_sender.send(self.video_id.clone()).unwrap();
		}
	}
	
	impl CachedAudioFile {
		pub fn file_mut(&mut self) -> &mut std::fs::File { &mut self.file }
	}
	
	fn video_id_to_audio_path<T: AsRef<str>>(video_id: T) -> std::path::PathBuf {
		let video_id = video_id.as_ref();
		let audio_filename = format!("{video_id}.mp3");
		let audio_path = AUDIO_CACHE_PATH.join(audio_filename);
		audio_path
	}
	
	pub async fn server(
		app_config: Arc<crate::AppConfig>,
		mut request_receiver: tokio::sync::mpsc::UnboundedReceiver<(tokio::sync::oneshot::Sender<CachedAudioFile>, String)>,
		audio_download_request_sender: mpsc::UnboundedSender<(oneshot::Sender<Result<PathBuf, crate::audio_download::Error>>, String)>,
	) {
		// Set up database.
		let mut connection = rusqlite::Connection::open("audio.db").unwrap();
		connection.execute_batch("
			CREATE TABLE IF NOT EXISTS Audio
				( video_id TEXT PRIMARY KEY
			
				, size INTEGER
			
				, last_accessed_timestamp INTEGER
			);
		").unwrap();
		
		// Set up directory.
		std::fs::create_dir_all(AUDIO_CACHE_PATH.as_os_str()).unwrap();

		// Delete any audio files that are in the database, but not in the directory.
		{
			let mut statement = connection.prepare(
				"SELECT video_id FROM Audio"
			).unwrap();

			let video_ids: Vec<String> = statement.query_map(
				rusqlite::params![],
				|row| Ok(row.get(0)?),
			).unwrap().map(|row| row.unwrap()).collect();
			
			std::mem::drop(statement);
			
			let transaction = connection.transaction().unwrap();
			for video_id in video_ids {
				let audio_path = video_id_to_audio_path(&video_id);
				let metadata = std::fs::metadata(audio_path);
				let file_exists = {
					if let Err(err) = metadata {
						if let std::io::ErrorKind::NotFound = err.kind() {
							false
						} else {
							panic!();
						}
					} else {
						true
					}
				};
				if !file_exists {
					transaction.execute(
						"DELETE FROM Audio WHERE audio_id = ?",
						rusqlite::params![video_id],
					).unwrap();
				}
			}
			transaction.commit().unwrap();
		}

		let connection: Arc<Mutex<rusqlite::Connection>> = Arc::new(Mutex::new(connection));

		let (drop_sender, mut drop_receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
		let audio_reference_count: Arc<Mutex<HashMap<String, u32>>> = Arc::new(Mutex::new(HashMap::new()));
		let audio_download_notify: Arc<Mutex<HashMap<String, Arc<Notify>>>> = Arc::new(Mutex::new(HashMap::new()));
		
		loop {
			tokio::select! {
				// NOTE: Make sure to always serve all audio requests, before handling the
				// drops, so that we don't delete an audio file which was actually
				// requested.
			
				biased;

				request = request_receiver.recv() => {
					let connection = connection.clone();
					let audio_download_notify = audio_download_notify.clone();
					let youtube_download_request_sender = audio_download_request_sender.clone();
					let audio_reference_count = audio_reference_count.clone();
					let drop_sender = drop_sender.clone();
					tokio::spawn(async move {
						let (response_sender, video_id) = request.unwrap();
						dbg!(&video_id);

						let audio_downloaded = {
							connection.lock().await.query_row(
								"SELECT EXISTS (SELECT 1 FROM Audio WHERE video_id = ?)",
								rusqlite::params![video_id],
								|row| row.get::<_, bool>(0),
							).unwrap()
						};
					
						let final_filename = format!("{}.mp3", &video_id);
						let final_path = std::path::Path::new(".")
							.join(AUDIO_CACHE_PATH.as_os_str())
							.join(&final_filename);
						
						let mut update_last_accessed_timestamp = true;
						
						if !audio_downloaded {
							let mut audio_download_notify = audio_download_notify.lock().await;
							if let Some(notify) = audio_download_notify.get(&video_id) {
								let notify = notify.clone();
								std::mem::drop(audio_download_notify);
								notify.notified().await;
							} else {
								update_last_accessed_timestamp = false;
								
								let notify = Arc::new(Notify::new());
								audio_download_notify.insert(video_id.clone(), notify.clone());
								std::mem::drop(audio_download_notify);
								
								let (response_sender, response_receiver) = oneshot::channel();
								youtube_download_request_sender.send((
									response_sender,
									video_id.clone(),
								)).unwrap();
								
								match response_receiver.await.unwrap() {
									Ok(response_path) => {
										let metadata = std::fs::metadata(&response_path).unwrap();
										
										// NOTE: Audio should always be first added to the database, and only
										// then moved to the directory, because in case the program is
										// unexpectedely stopped, it will only delete database entries without
										// corresponding audio files (and not audio files without
										// corresponding database entries).
								
										// Add audio to database.
										connection.lock().await.execute(
											"INSERT INTO Audio (video_id, size, last_accessed_timestamp) VALUES (?, ?, ?)",
											rusqlite::params![video_id, metadata.len(), chrono::Utc::now()],
										).unwrap();

										// Move audio to cache.
										std::fs::rename(&response_path, &final_path).unwrap();
									}
									Err(err) => {
										// TODO: log
										// TODO: broadcast to all waiters that download failed
									}
								}
								
								notify.notify_waiters();
							}
						}
						
						if update_last_accessed_timestamp {
							connection.lock().await.execute(
								"UPDATE Audio SET last_accessed_timestamp = ? WHERE video_id = ?",
								rusqlite::params![chrono::Utc::now(), video_id],
							).unwrap();
						}
						
						{
							let mut audio_reference_count = audio_reference_count.lock().await;
							// Inefficient entry api :(.
							let references = audio_reference_count.entry(video_id.clone()).or_insert(0);
							*references += 1;
							std::mem::drop(audio_reference_count);
						}
						
						let result = response_sender.send(
							CachedAudioFile {
								file: std::fs::File::open(final_path).unwrap(),
								drop_sender,
								video_id,
							}
						);
						
						if let Err(_) = result {
							// TODO: log.
						}
					});
				}

				video_id = drop_receiver.recv() => {
					let video_id = video_id.unwrap();
					let mut audio_reference_count = audio_reference_count.lock().await;
					let references = audio_reference_count.get_mut(&video_id).unwrap();
					*references -= 1;
					if *references == 0 {
						audio_reference_count.remove(&video_id);
					}
				}
			}
			
			// Clean up cache.
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
			
			let audio_reference_count = audio_reference_count.lock().await;
			audios.retain(|audio| !audio_reference_count.contains_key(&audio.video_id));
			
			let mut audio_cache_size: u64 = audios.iter().map(|audio| audio.size).sum();
		
			if audio_cache_size > app_config.audio_cache_size {
				while !audios.is_empty()
					&& audio_cache_size > app_config.audio_cache_size
				{
					let audio = audios.pop().unwrap();
					let audio_path = video_id_to_audio_path(audio.video_id);
					audio_cache_size -= audio.size;
					std::fs::remove_file(&audio_path).unwrap();
				}
			}

			std::mem::drop(audio_reference_count);
		}
	}
}

#[tokio::main]
async fn main() {
	let args = Args::parse();
	
	let youtube_secret = google_youtube3::oauth2::read_service_account_key(args.youtube_secret_path).await.unwrap();
	
	std::env::set_current_dir(args.working_dir).unwrap();

	// NOTE: It should theoretically be possible to replace a lot of the Arcs in
	// this code with immutable references, but I think that rust async is not good
	// enough for that to work yet.
	
	let app_config = AppConfig {
		audio_cache_size: 1024 * 1024 * 100,
		domain: String::from("192.168.1.139"),
		youtube_update_delay: chrono::Duration::minutes(15),
	};
	let app_config = Arc::new(app_config);

	let (audio_download_request_sender, audio_download_request_receiver) = tokio::sync::mpsc::unbounded_channel();
	let (audio_cache_request_sender, audio_cache_request_receiver) = tokio::sync::mpsc::unbounded_channel();
	let (youtube_data_request_sender, youtube_data_request_receiver) = tokio::sync::mpsc::unbounded_channel();
	
	let make_service = {
		let audio_request_sender = audio_cache_request_sender.clone();
		let youtube_data_request_sender = youtube_data_request_sender.clone();
		let app_config = app_config.clone();
		hyper::service::make_service_fn(move |_conn: &hyper::server::conn::AddrStream| {
			let audio_request_sender = audio_request_sender.clone();
			let youtube_data_request_sender = youtube_data_request_sender.clone();
			let app_config = app_config.clone();
			async move {
				anyhow::Ok(
					hyper::service::service_fn(move |request| {
						let audio_request_sender = audio_request_sender.clone();
						let youtube_data_request_sender = youtube_data_request_sender.clone();
						let app_config = app_config.clone();
						println!("got a request!");
						web::server(
							app_config,
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
			let result = server.await.unwrap();
		},
		audio_download::server(
			app_config.clone(),
			audio_download_request_receiver,
		),
		audio_cache::server(
			app_config.clone(),
			audio_cache_request_receiver,
			audio_download_request_sender,
		),
		// TODO: Get highest quality channel picture.
		youtube_data::server(
			app_config.clone(),
			youtube_secret,
			youtube_data_request_receiver,
		),
	};
}
