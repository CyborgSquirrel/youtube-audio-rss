use std::{sync::Arc, io::Read};

use clap::Parser;

#[derive(Debug, clap::Parser)]
struct Args {
	#[arg(long)]
	config_path: std::path::PathBuf,
	#[arg(long, default_value_t = log::Level::Error)]
	log: log::Level,
}

// NOTE: This is quite ugly, I know, it's not great that I used unwrap here, but
// I just sorta wanna be done already :).

fn deserialize_duration<'de, D>(deserializer: D) -> Result<chrono::Duration, D::Error> where D: serde::Deserializer<'de> {
	serde_humanize_rs::deserialize::<std::time::Duration, D>(deserializer)
		.map(|duration| chrono::Duration::from_std(duration).unwrap())
}

fn deserialize_size<'de, D>(deserializer: D) -> Result<u64, D::Error> where D: serde::Deserializer<'de> {
	serde_humanize_rs::deserialize::<usize, D>(deserializer)
		.map(|size| size.try_into().unwrap())
}

#[derive(serde::Deserialize)]
pub struct Config {
	ip: std::net::Ipv4Addr,
	port: u16,

	working_dir: std::path::PathBuf,
	youtube_secret_path: std::path::PathBuf,
	
	#[serde(deserialize_with = "deserialize_size")]
	audio_cache_size: u64,
	#[serde(deserialize_with = "deserialize_duration")]
	youtube_update_delay: chrono::Duration,
	
	url_scheme: String,
	url_path_prefix: String,
}

mod web {
    use std::{io::Read, sync::Arc};

    use tokio::sync::{mpsc, oneshot};

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

	async fn get_feed<T: AsRef<str>>(
		config: Arc<crate::Config>,
		youtube_data_request_sender: mpsc::UnboundedSender<(oneshot::Sender<crate::youtube_data::Channel>, crate::youtube_data::ChannelIdentifier)>,
		channel_identifier: &crate::youtube_data::ChannelIdentifier,
		host: T,
	) -> String {
		// TODO: Maybe add some of the following tags:
		// itunes:author
		// itunes:duration
		// itunes:explicit
		
		let host = host.as_ref();
		let url_scheme = &config.url_scheme;
		let url_path_prefix = &config.url_path_prefix;
		
		let (response_sender, response_receiver) = oneshot::channel();
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
					if let Some(image_url) = &channel.image_url {
						writer.nest(
							xml::writer::XmlEvent::start_element("itunes:image")
								.attr(xml::name::Name::local("href"), image_url),
							|_| Ok(())
						)?;
					}
					
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
								
								let enclosure_href = format!("{url_scheme}://{host}{url_path_prefix}/audio/{video_id}.mp3");
								
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
		config: Arc<crate::Config>,
		audio_cache_request_sender: mpsc::UnboundedSender<(oneshot::Sender<Result<crate::audio_cache::CachedAudioFile, ()>>, String)>,
		youtube_data_request_sender: mpsc::UnboundedSender<(oneshot::Sender<crate::youtube_data::Channel>, crate::youtube_data::ChannelIdentifier)>,
		request: hyper::Request<hyper::Body>,
	) -> anyhow::Result<hyper::Response<hyper::Body>> {
		fn error_response(error: hyper::StatusCode) -> hyper::Response<hyper::Body> {
			hyper::Response::builder()
				.status(error)
				.body(hyper::Body::from(Vec::new()))
				.unwrap()
		}

		let Some(host) = request.headers().get(hyper::header::HOST) else {
			return Ok(error_response(hyper::StatusCode::BAD_REQUEST));
		};
		let host = host.to_str().unwrap();
		
		let path = std::path::Path::new(request.uri().path());
		if let &hyper::Method::GET = request.method() {
			if let Some(parent) = path.parent().map(|parent| parent.as_os_str()).and_then(|parent| parent.to_str()) {
				match parent {
					"/audio" => {
						let Some("mp3") = path.extension().and_then(|extension| extension.to_str()) else {
							return Ok(error_response(hyper::StatusCode::NOT_FOUND));
						};

						let Some(video_id) = path.file_stem().and_then(|stem| stem.to_str()) else {
							return Ok(error_response(hyper::StatusCode::NOT_FOUND));
						};

						let (response_sender, response_receiver) = oneshot::channel();
						audio_cache_request_sender.send((
							response_sender,
							String::from(video_id),
						)).unwrap();
	
						let cached_audio_file = response_receiver.await.unwrap();

						if let Ok(mut cached_audio_file) = cached_audio_file {
							let mut audio = Vec::new();
							cached_audio_file.file_mut().read_to_end(&mut audio).unwrap();
							anyhow::Ok(
								hyper::Response::builder()
									.header(hyper::header::CONTENT_TYPE, "audio/mp3")
									.body(hyper::Body::from(audio))
									.unwrap())
						} else {
							anyhow::Ok(error_response(hyper::StatusCode::INTERNAL_SERVER_ERROR))
						}
					}
					"/feed/channel_id" => {
						let Some(channel_id) = path.file_name()
							.and_then(|file_name| file_name.to_str())
							.map(|file_name| file_name.to_string())
						else {
							return anyhow::Ok(error_response(hyper::StatusCode::NOT_FOUND));
						};

						let feed = get_feed(
							config,
							youtube_data_request_sender,
							&crate::youtube_data::ChannelIdentifier::Id(channel_id),
							host,
						).await;

						anyhow::Ok(
							hyper::Response::builder()
								.header(hyper::header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")
								.body(hyper::Body::from(feed))
								.unwrap()
						)
					}
					"/feed/channel_custom_url" => {
						let Some(channel_custom_url) = path.file_name()
							.and_then(|file_name| file_name.to_str())
							.map(|file_name| file_name.to_string())
						else {
							return anyhow::Ok(error_response(hyper::StatusCode::NOT_FOUND));
						};

						let feed = get_feed(
							config,
							youtube_data_request_sender,
							&crate::youtube_data::ChannelIdentifier::CustomUrl(channel_custom_url),
							host,
						).await;

						anyhow::Ok(
							hyper::Response::builder()
								.header(hyper::header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")
								.body(hyper::Body::from(feed))
								.unwrap()
						)
					}
					_ => {
						anyhow::Ok(error_response(hyper::StatusCode::NOT_FOUND))
					}
				}
			} else {
				anyhow::Ok(error_response(hyper::StatusCode::NOT_FOUND))
			}
		} else {
			anyhow::Ok(error_response(hyper::StatusCode::NOT_FOUND))
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
		pub image_url: Option<String>,
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
		config: Arc<crate::Config>,
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
			let config = config.clone();
			tokio::spawn(async move {
				let mut connection = connection.lock().await;
				
				let channel_id = {
					update_channel_data(
						config.clone(),
						&mut connection,
						&youtube_hub,
						&channel_identifier,
					).await.unwrap()
				};
				
				struct ChannelMetadata {
					username: String,
					description: String,
					image_url: Option<String>,
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

				let result = sender.send(channel);
				
				if let Err(_) = result {
					log::info!("couldn't send channel data");
				}
			});
		}
	}
	
	#[derive(Debug)]
	pub enum Error {
		InvalidChannelId,
		InvalidChannelCustomUrl,
	}
	
	async fn update_channel_data(
		config: Arc<crate::Config>,
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
						
						// TODO: Do something about this get failling.
						let response = client.get(uri).await.unwrap();
						let (_head, body) = response.into_parts();
						let bytes = hyper::body::to_bytes(body).await.unwrap();
						let text = std::str::from_utf8(&bytes).unwrap();
						
						lazy_static::lazy_static! {
							static ref channel_id_regex: regex::Regex = {
								let regex_source = r#"<link rel="alternate" type="application/rss[+]xml" title="RSS" href="https://www[.]youtube[.]com/feeds/videos[.]xml[?]channel_id=([^"]*)">"#;
								let regex = regex::RegexBuilder::new(regex_source).build().unwrap();
								regex
							};
						}
						
						let channel_id = channel_id_regex.captures(text).map(|capture| capture.get(1).unwrap().as_str().to_string());
						
						if let Some(channel_id) = channel_id {
							channel_id
						} else {
							return Err(Error::InvalidChannelCustomUrl);
						}
					}
				}
			}
		};
		
		update_channel_data_sync(
			tokio::runtime::Handle::current(),
			connection,
			&youtube_hub,
			config.youtube_update_delay,
			&channel_id,
		)
			.map(|_| channel_id)
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
			let thumbnails = snippet.thumbnails.as_ref().unwrap();
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
					thumbnails.high.as_ref()
						.or(thumbnails.medium.as_ref())
						.or(thumbnails.standard.as_ref())
						.map(|thumbnail| thumbnail.url.as_ref().unwrap()),
					chrono::Utc::now(),
				],
			).unwrap();
			
			// Get videos data.
			let mut page_token: Option<String> = None;
			'downloading: loop {
				let (_, results) = {
					let request = {
						youtube_hub.playlist_items()
							.list(&vec!["id".to_string(), "snippet".to_string()])
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
				
				page_token = results.next_page_token;
			
				let items = results.items.unwrap();
				for item in items.iter() {
					let snippet = item.snippet.as_ref().unwrap();
					let thumbnails = snippet.thumbnails.as_ref().unwrap();
					let result = transaction.execute(
						"INSERT INTO Video (id, channel_id, name, description, image_url, published_timestamp) VALUES (?, ?, ?, ?, ?, ?)",
						rusqlite::params![
							snippet.resource_id.as_ref().unwrap().video_id.as_ref().unwrap(),
							snippet.channel_id.as_ref().unwrap(),
							snippet.title.as_ref().unwrap(),
							snippet.description.as_ref().unwrap(),
							thumbnails.high.as_ref()
								.or(thumbnails.medium.as_ref())
								.or(thumbnails.standard.as_ref())
								.map(|thumbnail| thumbnail.url.as_ref().unwrap()),
							snippet.published_at.as_ref().unwrap(),
						],
					);
					
					// TODO: Extended_code should really be
					// rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY, but for some reason that
					// constant doesn't show up if I don't add the "bundled" feature in
					// rusqlite.
					//
					// I really prefer not using "bundled" though, so for now I'll leave it
					// like this. Hopefully in the future this gets fixed.
					if let Err(rusqlite::Error::SqliteFailure(rusqlite::ffi::Error { code: rusqlite::ffi::ErrorCode::ConstraintViolation, extended_code: _ }, _)) = result {
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
		static ref AUDIO_TMP_PATH: &'static std::path::Path = std::path::Path::new("./audio_tmp");
		static ref AUDIO_OUTPUT_PATH: &'static std::path::Path = std::path::Path::new("./audio_output");
	}
	
	#[derive(Debug)]
	pub enum Error {
		AlreadyDownloading,
		YtDlpError(String),
	}
	
	pub async fn server(
		config: Arc<crate::Config>,
		mut request_receiver: mpsc::UnboundedReceiver<(oneshot::Sender<Result<PathBuf, Error>>, String)>,
	) {
		if AUDIO_TMP_PATH.exists() {
			std::fs::remove_dir_all(&*AUDIO_TMP_PATH).unwrap();
		}
		std::fs::create_dir_all(&*AUDIO_TMP_PATH).unwrap();

		if AUDIO_OUTPUT_PATH.exists() {
			std::fs::remove_dir_all(&*AUDIO_OUTPUT_PATH).unwrap();
		}
		std::fs::create_dir_all(&*AUDIO_OUTPUT_PATH).unwrap();

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

					let audio_output_path = AUDIO_OUTPUT_PATH.join(&audio_filename);
					let tmp_path = AUDIO_TMP_PATH.join(&video_id);
			
					let audio_tmp_path = tmp_path
						.join(&audio_filename);
					
					// Run command to download audio.
					let output = async_log::span!(
						"running command to download video, video_id={}", &video_id, {
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
					});
			
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

    use tokio::sync::{Mutex, mpsc, oneshot, broadcast};

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
	
	// TODO: Test to make sure that the reference-counting works correctly.
	pub async fn server(
		config: Arc<crate::Config>,
		mut request_receiver: tokio::sync::mpsc::UnboundedReceiver<(tokio::sync::oneshot::Sender<Result<CachedAudioFile, ()>>, String)>,
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
				let file_exists = audio_path.exists();
				if !file_exists {
					transaction.execute(
						"DELETE FROM Audio WHERE video_id = ?",
						rusqlite::params![video_id],
					).unwrap();
				}
			}
			transaction.commit().unwrap();
		}

		let connection = Arc::new(Mutex::<rusqlite::Connection>::new(connection));

		let (drop_sender, mut drop_receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
		let audio_reference_count = Arc::new(Mutex::new(HashMap::<String, u32>::new()));
		let audio_available_notify = Arc::new(Mutex::new(HashMap::<String, broadcast::Sender<bool>>::new()));
		let (run_cleanup_sender, mut run_cleanup_receiver) = tokio::sync::mpsc::unbounded_channel::<()>();

		let task_reference_counter = {
			let audio_reference_count = audio_reference_count.clone();
			let connection = connection.clone();
			async move {
				loop {
					tokio::select! {
						// NOTE: Make sure to always serve all audio requests, before handling the
						// drops, so that we don't delete an audio file which was actually
						// requested.
		
						biased;

						request = request_receiver.recv() => {
							let connection = connection.clone();
							let audio_download_notify = audio_available_notify.clone();
							let youtube_download_request_sender = audio_download_request_sender.clone();
							let audio_reference_count = audio_reference_count.clone();
							let drop_sender = drop_sender.clone();
							let run_cleanup_sender = run_cleanup_sender.clone();
							tokio::spawn(async move {
								let (response_sender, video_id) = request.unwrap();

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
								let mut audio_available = true;
					
								if !audio_downloaded {
									let mut audio_download_notify = audio_download_notify.lock().await;
									if let Some(mut receiver) = audio_download_notify.get(&video_id).map(|sender| sender.subscribe()) {
										std::mem::drop(audio_download_notify);
										audio_available = receiver.recv().await.unwrap();
									} else {
										update_last_accessed_timestamp = false;
						
										let (broadcast_sender, _) = broadcast::channel(1);
										audio_download_notify.insert(video_id.clone(), broadcast_sender.clone());
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
										
												// Don't check whether send was succesful or not.
												let _ = run_cleanup_sender.send(());
											}
											Err(err) => {
												log::error!("failed to download youtube video audio {err:?}");
												audio_available = false;
											}
										};
						
										// We don't care if there were any/no receivers for the broadcast.
										let _ = broadcast_sender.send(audio_available);
									}
								}

								// TODO: I'm pretty sure that a file that has just been downloaded could
								// technically get cleaned up by the cache at this point. Find a way to
								// prevent that.
					
								if !audio_available {
									let _ = response_sender.send(Err(()));
								} else {
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
										Ok(
											CachedAudioFile {
												file: std::fs::File::open(final_path).unwrap(),
												drop_sender,
												video_id})
									);
					
									if let Err(_) = result {
										log::info!("couldn't send audio file");
									}
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
								// Don't check whether send was succesful or not.
								let _ = run_cleanup_sender.send(());
							}
						}
					}
				}
			}
		};
		
		let task_cleanup = {
			async move {
				while let Some(()) = run_cleanup_receiver.recv().await {
					// Clean up cache.
					#[derive(Debug)]
					struct Audio {
						video_id: String,
						size: u64,
					}
		
					let connection = connection.lock().await;
					
					// Audios sorted from least to most recently accessed.
					let mut audios: Vec<_> = {
						let mut statement = connection.prepare(
							"SELECT video_id, size FROM Audio ORDER BY last_accessed_timestamp ASC"
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
						rows
					};
					
					// Don't clean up the cache if there's only one audio file, even if that
					// file is over the limit.
					if audios.len() <= 1 {
						break;
					}

					let mut audio_cache_size: u64 = audios.iter().map(|audio| audio.size).sum();
			
					let audio_reference_count = audio_reference_count.lock().await;
					// Audios which are not currently opened.
					let unused_audios = {
						audios.retain(|audio| !audio_reference_count.contains_key(&audio.video_id));
						audios
					};
		
					for unused_audio in unused_audios {
						if !(audio_cache_size > config.audio_cache_size) {
							break;
						}

						let audio_path = video_id_to_audio_path(&unused_audio.video_id);
						audio_cache_size -= unused_audio.size;
						std::fs::remove_file(&audio_path).unwrap();
						connection.execute(
							"DELETE FROM Audio WHERE video_id = ?",
							rusqlite::params![unused_audio.video_id],
						).unwrap();
					}

					std::mem::drop(connection);
					std::mem::drop(audio_reference_count);
				}
			}
		};
		
		tokio::join! {
			task_reference_counter,
			task_cleanup,
		};
	}
}

// TODO: Sometimes a youtube channel will private/unlist some of their videos.
// It would be good if the program were able to refersh the whole video catalog
// every once in a while.

// TODO: Calculate the length of the videos, by subtracting the starting time
// from the ending time (in the context of the all videos playlist). Normally,
// this wouldn't be a 100% reliable way to get the length of a video, but I
// think that in this case it will be perfect, because I'm pretty sure the all
// videos playlist is auto-generated.

// TODO: Pagination for the rss feed.

// NOTE: It should theoretically be possible to replace a lot of the Arcs in
// this code with immutable references, but I think that rust async is not good
// enough for that to work yet.

#[tokio::main]
async fn main() {
	let args = Args::parse();
	
	simple_logger::init_with_level(args.log).unwrap();
	
	let config: Config = {
		let mut file = std::fs::File::open(&args.config_path).unwrap();
		let mut contents = String::new();
		file.read_to_string(&mut contents)
			.expect("Could not read config file.");
		toml::from_str(&contents)
			.expect("Could not parse config file.")
	};
	let config = Arc::new(config);
	
	let youtube_secret = google_youtube3::oauth2::read_service_account_key(&config.youtube_secret_path).await
		.expect("Could not open youtube secret.");
	
	if !config.working_dir.exists() {
		std::fs::create_dir_all(&config.working_dir).unwrap();
	}
	std::env::set_current_dir(&config.working_dir).unwrap();

	let (audio_download_request_sender, audio_download_request_receiver) = tokio::sync::mpsc::unbounded_channel();
	let (audio_cache_request_sender, audio_cache_request_receiver) = tokio::sync::mpsc::unbounded_channel();
	let (youtube_data_request_sender, youtube_data_request_receiver) = tokio::sync::mpsc::unbounded_channel();
	
	let make_service = {
		let audio_request_sender = audio_cache_request_sender.clone();
		let youtube_data_request_sender = youtube_data_request_sender.clone();
		let config = config.clone();
		hyper::service::make_service_fn(move |_conn: &hyper::server::conn::AddrStream| {
			let audio_request_sender = audio_request_sender.clone();
			let youtube_data_request_sender = youtube_data_request_sender.clone();
			let config = config.clone();
			async move {
				anyhow::Ok(
					hyper::service::service_fn(move |request| {
						let audio_request_sender = audio_request_sender.clone();
						let youtube_data_request_sender = youtube_data_request_sender.clone();
						let config = config.clone();
						log::info!("received an http request");
						web::server(
							config,
							audio_request_sender,
							youtube_data_request_sender,
							request,
						)
					})
				)
			}
		})
	};
	
	tokio::join! {
		async {
			let address = std::net::SocketAddr::from((config.ip, config.port));
			let server = hyper::Server::bind(&address).serve(make_service);
			let result = server.await.unwrap();
		},
		audio_download::server(
			config.clone(),
			audio_download_request_receiver,
		),
		audio_cache::server(
			config.clone(),
			audio_cache_request_receiver,
			audio_download_request_sender,
		),
		youtube_data::server(
			config.clone(),
			youtube_secret,
			youtube_data_request_receiver,
		),
	};
}
