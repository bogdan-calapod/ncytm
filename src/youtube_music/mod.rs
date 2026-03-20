//! YouTube Music API client and authentication.
//!
//! This module provides the core functionality for interacting with YouTube Music:
//! - Cookie-based authentication
//! - API client for making requests
//! - Response parsing
//! - Audio stream extraction

pub mod api;
pub mod auth;
pub mod client;
pub mod cookies;
pub mod stream;

pub use api::{
    Album, AlbumTrack, Artist, ArtistAlbum, ArtistTopSong, LibraryAlbum, LibraryPlaylist,
    LibraryResponse, LibraryTrack, Playlist, PlaylistResponse, SearchAlbum, SearchArtist,
    SearchPlaylist, SearchResults, SearchTrack, get_album, get_artist, get_library_albums,
    get_library_playlists, get_liked_songs, get_playlist, get_playlist_continuation, search,
};
pub use auth::{YOUTUBE_MUSIC_ORIGIN, generate_sapisid_hash};
pub use client::{AccountInfo, ClientError, YouTubeMusicClient};
pub use cookies::{CookieError, Cookies};
pub use stream::{AudioQuality, StreamError, StreamInfo, get_audio_streams, get_stream_url};
