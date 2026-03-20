//! YouTube Music API endpoints.
//!
//! This module contains functions for interacting with specific YouTube Music API endpoints.

pub mod album;
pub mod artist;
pub mod library;
pub mod playlist;
pub mod search;

pub use album::{get_album, Album, AlbumTrack};
pub use artist::{get_artist, Artist, ArtistAlbum, ArtistTopSong};
pub use library::{
    get_library_albums, get_library_playlists, get_liked_songs, LibraryAlbum, LibraryPlaylist,
    LibraryResponse, LibraryTrack,
};
pub use playlist::{get_playlist, get_playlist_continuation, Playlist, PlaylistResponse};
pub use search::{search, SearchAlbum, SearchArtist, SearchPlaylist, SearchResults, SearchTrack};
