<div align="center" style="text-align:center">

# ncytm

### An ncurses YouTube Music client written in Rust

</div>

> [!WARNING]
>
> This is a fork of [ncspot](https://github.com/hrkfdn/ncspot), an `ncurses` Spotify client.
>
> **AI-Assisted Development**: This project is being developed with heavy AI involvement. Code quality and functionality may vary. Use at your own risk.
>
> **macOS support mainly**: I'm forking this for my own personal use, based mostly on macOS. While best effort is intended, no guarantee of upstream fixes or other OS support is provided.

## About

ncytm is an `ncurses` YouTube Music client written in Rust. It is a fork of ncspot, adapted to work with YouTube Music instead of Spotify using cookie-based authentication (copy cookies from your browser).

ncytm aims to provide a simple and resource-friendly terminal interface for YouTube Music, inspired by ncurses MPD clients like [ncmpc](https://musicpd.org/clients/ncmpc/).

## Features

- Play tracks, albums, and playlists from YouTube Music
- Access your YouTube Music library (liked songs, playlists, albums, artists)
- Search for tracks, albums, artists, and playlists
- Small resource footprint
- Vim keybindings out of the box
- Cookie-based authentication (copy from browser)
- macOS media keys and Now Playing integration, with automatic reclaim of the Now Playing slot when a new track starts
- Optional Slack "now playing" status: appends the current track to your Slack status
- Smart previous track behavior: restarts current track if more than 15% played, otherwise goes to previous track

## Installation

### Homebrew (macOS)

```bash
brew tap bogdan-calapod/tap
brew trust --formula bogdan-calapod/tap/ncytm
brew install yt-dlp ffmpeg ncytm
```

### From Source

Building ncytm requires a working [Rust installation](https://www.rust-lang.org/tools/install).

```bash
git clone https://github.com/bogdan-calapod/ncytm.git
cd ncytm
cargo build --release
```

The binary will be at `target/release/ncytm`.

## Authentication

ncytm uses cookie-based authentication. The easiest way to authenticate is using the built-in browser authentication:

### Browser Authentication (Recommended)

```bash
ncytm auth --browser
```

This will open your default browser (Edge) where you can log in to YouTube Music. Once authenticated, cookies are automatically extracted and saved.

**Options:**

- `--browser-type <TYPE>` - Use a different browser: `chrome`, `edge`, or `chromium` (default: `edge`)
- `--no-system-profile` - Use a separate ncytm browser profile instead of your system profile
- `--timeout <SECONDS>` - Timeout for authentication (default: 600 seconds)

**Examples:**

```bash
# Use Chrome instead of Edge
ncytm auth --browser --browser-type chrome

# Check if your current cookies are still valid
ncytm auth --check
```

### Manual Cookie Export

Alternatively, you can manually export cookies from your browser:

1. Install a browser extension to export cookies (e.g., "Get cookies.txt LOCALLY" for Chrome/Firefox)
2. Go to [music.youtube.com](https://music.youtube.com) and sign in
3. Export cookies in Netscape format
4. Save the file to `~/.config/ncytm/cookies.txt`

The following cookies are required:

- `SAPISID` or `__Secure-3PAPISID`
- `HSID`
- `SSID`
- `APISID`
- `SID`
- `LOGIN_INFO`

## Configuration

Configuration files are stored in `~/.config/ncytm/`:

- `cookies.txt` - Your YouTube Music cookies (required)
- `config.toml` - Application configuration (optional)

### macOS Now Playing focus

On macOS, the Control Center "Now Playing" widget is owned by whichever app most
recently started playing. When another app (Spotify, Safari, Music, ...) plays
after ncytm, it takes over that slot.

ncytm automatically reclaims the Now Playing slot whenever a new track starts
playing — or when playback resumes from pause — so it stays the active player
without needing to quit and reopen the app. This happens only on track changes
and resume (never on a timer), so it won't fight other apps while a track is
playing. No configuration is required.

### Slack now-playing status

ncytm can append the currently playing track to your Slack status. It preserves
whatever status you already have and only touches Slack when the track changes,
stripping its addition again when playback is paused or stopped. For example, a
status of `Focusing` becomes `Focusing |🎵 Song — Artist` while playing.

Because ncytm only updates on track change, if you change your Slack status
manually while nothing is playing, ncytm leaves it alone and adopts it as the
new base the next time a track starts.

**Setup:**

1. Create a Slack app at <https://api.slack.com/apps> and add the
   `users.profile:read` and `users.profile:write` **User Token Scopes**.
2. Install the app to your workspace and copy the **User OAuth Token**
   (`xoxp-...`).
3. Provide the token either via the `SLACK_TOKEN` environment variable
   (recommended, keeps it out of your config file) or in `config.toml`.
4. Verify it with `ncytm slack --check`.

Example `config.toml`:

```toml
[slack_status]
enabled = true
# token = "xoxp-..."   # optional; prefer the SLACK_TOKEN env var
separator = "|"          # the marker ncytm uses to find/remove its addition
emoji = "🎵"             # placed inside the appended text, after the separator
format = "{title} — {artists}"
```

Your Slack `status_emoji` is never modified — the music emoji lives inside the
appended text. Slack limits status text to 100 characters; if the combined
status is too long, only ncytm's addition is truncated, never your base status.

## Requirements

- **yt-dlp**: Required for audio playback. Install via `brew install yt-dlp` or `pip install yt-dlp`
- *ffmpeg*: Required for converting the downloaded audio in a default format. Install via `brew
install ffmpeg`

## Credits

This project is a fork of [ncspot](https://github.com/hrkfdn/ncspot) by hrkfdn. Many thanks to the original authors and contributors.

## License

Same license as the original ncspot project - see [LICENSE](LICENSE) file.
