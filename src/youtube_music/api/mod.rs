//! YouTube Music API endpoints.
//!
//! This module contains functions for interacting with specific YouTube Music API endpoints.

pub mod search;

pub use search::{search, SearchResults, SearchTrack, SearchAlbum, SearchArtist, SearchPlaylist};
