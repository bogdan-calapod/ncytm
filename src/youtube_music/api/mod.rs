//! YouTube Music API endpoints.
//!
//! This module contains functions for interacting with specific YouTube Music API endpoints.

pub mod album;
pub mod artist;
pub mod browse;
pub mod library;
pub mod playlist;
pub mod radio;
pub mod search;

pub use album::{AlbumDetails, AlbumTrack, get_album};
pub use artist::{ArtistAlbum, ArtistDetails, ArtistTrack, get_artist};
pub use browse::{get_categories, get_category_playlists};
pub use library::{
    LibraryAlbum, LibraryPlaylist, LibraryTrack, get_library_albums, get_library_playlists,
    get_liked_songs,
};
pub use playlist::{
    PlaylistTrack, add_playlist_tracks, create_playlist, delete_playlist, follow_playlist,
    get_playlist_info, get_playlist_tracks, remove_playlist_tracks, unfollow_playlist,
};
pub use radio::{RadioTrack, get_radio, get_radio_continuation};
pub use search::{SearchAlbum, SearchArtist, SearchPlaylist, SearchResults, SearchTrack, search};
