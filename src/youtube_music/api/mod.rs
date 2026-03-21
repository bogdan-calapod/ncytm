//! YouTube Music API endpoints.
//!
//! This module contains functions for interacting with specific YouTube Music API endpoints.

pub mod library;
pub mod radio;
pub mod search;

pub use library::{
    LibraryAlbum, LibraryPlaylist, LibraryTrack, get_library_albums, get_library_playlists,
    get_liked_songs,
};
pub use radio::{RadioTrack, get_radio};
pub use search::{SearchAlbum, SearchArtist, SearchPlaylist, SearchResults, SearchTrack, search};
