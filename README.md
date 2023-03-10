# youtube-channel-podcast

Turn youtube channels into podcast rss feeds.

## Usage

After setting it up, go to `{host}/feed/channel_id/{channel_id}`, or
`{host}/feed/channel_custom_url/{channel_custom_url}` to get the rss feed
corresponding to the youtube channel whose id is `channel_id`, or whose custom
url is `channel_custom_url` respectively.

The custom url of a channel is a string which starts with an `@`, and which can
take you to that channel's page, if you go to
`https://www.youtube.com/{custom_url}`. For example, `@YouTube` is YouTube's
custom channel url, and you can go to YouTube's own channel by visiting
[https://www.youtube.com/@YouTube](https://www.youtube.com/@YouTube).

## Installation

### If you're using nix flakes

Add the following input:

```nix
inputs = {
  # ...
  youtube-channel-podcast.url = "https://github.com/CyborgSquirrel/youtube-channel-podcast.git";
};
```

Then add the following lines in your nixos configuration:

```nix
services.youtube-channel-podcast = {
  enable = true;
  config = ''
    # config format described below
  '';
};
```

### If you're not using nix flakes

- [install rust](https://www.rust-lang.org/tools/install)
- run `cargo build --release` in the root of this repo
- copy the executable file at `/target/release/youtube-channel-podcast` to
  wherever you want
- run the application with the command
  `./youtube-channel-podcast --config-path /path/to/config_file.toml`

Configuration file format is described below.

## Configuration

The configuration file format is toml. Here is a sample config, with all the
options commented.

```toml
# ip and port to which the server will bind
ip = "0.0.0.0"
port = 4100

# directory where all the application's data will be stored
working_dir = "/path/to/working_dir"

# path to a youtube api key
youtube_secret_path = "/path/to/youtube_secret.json"

# maximum amount of storage that cached audio files can take up
audio_cache_size = "100M"

# minimum amount of time to wait, before updating a youtube channel's feed
# NOTE: a channel's feed will only ever be updated when it is requested; the
# application does not try to update feeds when it's not necessary
youtube_update_delay = "15m"

# the scheme of the audio urls in the generated rss feed
url_scheme = "http"

# a prefix which will be added to the audio urls in the generated rss feed
url_path_prefix = ""

# NOTE: you may want to use this, if you're trying to run this service behind a
# reverse proxy
# url_path_prefix = "/youtube_channel_podcast"
```
