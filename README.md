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
- macOS media keys and Now Playing integration

## Installation

### Homebrew (macOS)

```bash
brew tap bogdan-calapod/tap
brew install ncytm
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

## Requirements

- **yt-dlp**: Required for audio playback. Install via `brew install yt-dlp` or `pip install yt-dlp`

## Credits

This project is a fork of [ncspot](https://github.com/hrkfdn/ncspot) by hrkfdn. Many thanks to the original authors and contributors.

## License

Same license as the original ncspot project - see [LICENSE](LICENSE) file.
