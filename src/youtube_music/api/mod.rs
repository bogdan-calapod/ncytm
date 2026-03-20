//! YouTube Music API endpoints.
//!
//! This module contains functions for interacting with specific YouTube Music API endpoints.

pub mod album;
pub mod artist;
pub mod library;
pub mod playlist;
pub mod search;

pub use album::{Album, AlbumTrack, get_album};
pub use artist::{Artist, ArtistAlbum, ArtistTopSong, get_artist};
pub use library::{
    LibraryAlbum, LibraryPlaylist, LibraryResponse, LibraryTrack, get_library_albums,
    get_library_playlists, get_liked_songs,
};
pub use playlist::{Playlist, PlaylistResponse, get_playlist, get_playlist_continuation};
pub use search::{SearchAlbum, SearchArtist, SearchPlaylist, SearchResults, SearchTrack, search};
