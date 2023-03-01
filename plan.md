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

# Notes

Wanna try to use youtube api, instead of the channels' rss feeds. I have
to request and store all the video ids.

The plan is:

- when you get a request for an rss feed:
	- if that channel is not in the db, then:
		- make a request for that channel, and add it to the db
	- if you haven't received a request for that feed in the last
	15 minutes, then:
		- update the videos of the feed's channel
		- to do this, request all videos from newest to oldest,
		page by page
		- add all the videos from the response into the db
		- whenever the response contains a video that is already
		in the db, stop the requests
	- generate the feed and return it

Channel
	- `id`
	- `name`
	- `description`
	- `image_url`
	- has all video information been downloaded when first accessed?

to get video thumbnail: `https://i.ytimg.com/vi/{video_id}/hq720.jpg`

Video
	- `id`
	- `channel_id`
	- `published_date`
	- `name`
	- `description`

# Notes 2

Set a limit on the amount of bytes the audio cache can take up. 500MB

When to delete audio files from the cache?

After you download a file, count the total amount of bytes all the audio
files are currently taking up. If it's greater than the limit, delete
the least-recently-accessed ones, one by one, until the total is under
the limit.
