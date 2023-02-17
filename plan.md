# What this program will do

Run an https server.

The https server will accept 2 types of requests:

1. request for the audio of a youtube video

2. request for an rss feed of all the videos from a youtube
   channel, the items of the feed being all the videos of the youtube
   channel, enclosing a link to the audio version of the video (using
   the first type of request)

# Youtube video audio request

This request will be performed using a url with the path:

- `/audio/{video_id}.mp3`

The audio will be downloaded using `yt-dlp`.

The downloaded audio of a particular video will be kept around for a day,
after which it will be deleted.

# Youtube channel rss feed request

This request will be performed using a url with either one of these paths:

- `/feed/channel_id/{channel_id}`

- `/feed/channel_username/{channel_username}`

If the request is made with the `channel_username`, then the youtube
api will be used to get the `channel_id`.

The channel's rss feed can be retrieved from:

`https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}`

From this rss feed should be extracted the title, description, (maybe)
thumbnail, and video url. Then the video url should be converted to an
audio request url, and put into a `link` tag with `rel="enclosure"`.

Note: to correctly convert the video url to an audio request url, I will
have to be careful with what I put in the host part of the url.

Ideally, I would have my own domain, and use that. Otherwise, I could
also use the same host that was used to make the request for the rss
feed (although that would mean that the feed will change anytime the
host changes, which seems undesirable).
