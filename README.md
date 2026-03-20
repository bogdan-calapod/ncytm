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

## Features (Planned)

- Support for tracks, albums, playlists, searching...
- Small resource footprint
- Vim keybindings out of the box
- Cookie-based authentication (copy from browser)
- Access to your YouTube Music library and playlists

## Authentication

ncytm uses cookie-based authentication. You'll need to copy your YouTube Music cookies from your browser to authenticate. Detailed instructions will be provided once this feature is implemented.

## Installation

*Coming soon*

## Configuration

A configuration file can be provided. The default location is `~/.config/ncytm`. Detailed configuration information will be available once the project matures.

## Building

Building ncytm requires a working [Rust installation](https://www.rust-lang.org/tools/install) and a Python 3 installation. To compile ncytm, run `cargo build`.

## Credits

This project is a fork of [ncspot](https://github.com/hrkfdn/ncspot) by hrkfdn. Many thanks to the original authors and contributors.

## License

Same license as the original ncspot project - see [LICENSE](LICENSE) file.
