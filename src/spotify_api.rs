//! Stub module for Spotify Web API functionality.
//! This will be replaced with YouTube Music API implementation.

use log::info;

use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::category::Category;
use crate::model::episode::Episode;
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::model::show::Show;
use crate::model::track::Track;
use crate::ui::pagination::{ApiPage, ApiResult};

/// API page with next cursor for pagination.
#[derive(Clone, Debug, Default)]
pub struct CursorPage<T> {
    pub items: Vec<T>,
    pub next: Option<String>,
}

/// Saved album wrapper for API responses.
#[derive(Clone, Debug)]
pub struct SavedAlbum {
    pub album: Album,
}

impl From<&SavedAlbum> for Album {
    fn from(saved: &SavedAlbum) -> Self {
        saved.album.clone()
    }
}

/// Saved track wrapper for API responses.
#[derive(Clone, Debug)]
pub struct SavedTrack {
    pub track: Track,
}

impl From<&SavedTrack> for Track {
    fn from(saved: &SavedTrack) -> Self {
        saved.track.clone()
    }
}

/// Saved show wrapper for API responses.
#[derive(Clone, Debug)]
pub struct SavedShow {
    pub show: Show,
}

impl From<&SavedShow> for Show {
    fn from(saved: &SavedShow) -> Self {
        saved.show.clone()
    }
}

/// API page for saved tracks.
#[derive(Clone, Debug, Default)]
pub struct SavedTracksPage {
    pub items: Vec<SavedTrack>,
    pub next: Option<String>,
}

/// API page for saved albums.  
#[derive(Clone, Debug, Default)]
pub struct SavedAlbumsPage {
    pub items: Vec<SavedAlbum>,
    pub next: Option<String>,
}

/// API page for saved shows.
#[derive(Clone, Debug, Default)]
pub struct SavedShowsPage {
    pub offset: u32,
    pub items: Vec<SavedShow>,
    pub next: Option<String>,
}

/// Stub WebApi - will be replaced with YouTube Music API.
#[derive(Clone, Default)]
pub struct WebApi {}

impl WebApi {
    pub fn new() -> Self {
        Self {}
    }

    /// Refresh the API token if needed.
    pub fn update_token(&self) -> Option<impl std::future::Future<Output = Result<(), String>>> {
        // Stub: no token refresh needed
        None::<std::future::Ready<Result<(), String>>>
    }

    // Library methods
    pub fn current_user_saved_tracks(&self, _offset: u32) -> Result<SavedTracksPage, String> {
        Ok(SavedTracksPage::default())
    }

    pub fn current_user_saved_tracks_add(&self, _ids: Vec<&str>) -> Result<(), String> {
        info!("save tracks stubbed");
        Ok(())
    }

    pub fn current_user_saved_tracks_delete(&self, _ids: Vec<&str>) -> Result<(), String> {
        info!("delete tracks stubbed");
        Ok(())
    }

    pub fn current_user_saved_albums(&self, _offset: u32) -> Result<SavedAlbumsPage, String> {
        Ok(SavedAlbumsPage::default())
    }

    pub fn current_user_saved_albums_add(&self, _ids: Vec<&str>) -> Result<(), String> {
        info!("save albums stubbed");
        Ok(())
    }

    pub fn current_user_saved_albums_delete(&self, _ids: Vec<&str>) -> Result<(), String> {
        info!("delete albums stubbed");
        Ok(())
    }

    pub fn current_user_playlist(&self) -> ApiResult<Playlist> {
        ApiResult::new(50, std::sync::Arc::new(|_offset| None))
    }

    pub fn current_user_followed_artists(
        &self,
        _cursor: Option<&str>,
    ) -> Result<CursorPage<Artist>, String> {
        Ok(CursorPage::default())
    }

    pub fn user_follow_artists(&self, _ids: Vec<&str>) -> Result<(), String> {
        info!("follow artists stubbed");
        Ok(())
    }

    pub fn user_unfollow_artists(&self, _ids: Vec<&str>) -> Result<(), String> {
        info!("unfollow artists stubbed");
        Ok(())
    }

    pub fn get_saved_shows(&self, _offset: u32) -> Result<SavedShowsPage, String> {
        Ok(SavedShowsPage::default())
    }

    pub fn save_shows(&self, _ids: &[&str]) -> Result<(), String> {
        info!("save shows stubbed");
        Ok(())
    }

    pub fn unsave_shows(&self, _ids: &[&str]) -> Result<(), String> {
        info!("unsave shows stubbed");
        Ok(())
    }

    // Content retrieval
    pub fn track(&self, _id: &str) -> Option<Track> {
        None
    }

    pub fn album(&self, _id: &str) -> Result<Album, String> {
        Err("Album stub".to_string())
    }

    pub fn artist(&self, _id: &str) -> Option<Artist> {
        None
    }

    pub fn artist_albums(&self, _id: &str, _limit: u32, _offset: u32) -> ApiPage<Album> {
        ApiPage {
            offset: 0,
            total: 0,
            items: Vec::new(),
        }
    }

    pub fn artist_top_tracks(&self, _id: &str) -> Vec<Track> {
        Vec::new()
    }

    pub fn artist_related_artists(&self, _id: &str) -> Vec<Artist> {
        Vec::new()
    }

    pub fn playlist(&self, _id: &str) -> Option<Playlist> {
        None
    }

    pub fn playlist_tracks(
        &self,
        _id: &str,
        _limit: u32,
        _offset: u32,
    ) -> Option<ApiPage<Playable>> {
        None
    }

    pub fn episode(&self, _id: &str) -> Option<Episode> {
        None
    }

    pub fn show(&self, _id: &str) -> Option<crate::model::show::Show> {
        None
    }

    pub fn show_episodes(&self, _id: &str, _offset: u32) -> ApiPage<Episode> {
        ApiPage {
            offset: 0,
            total: 0,
            items: Vec::new(),
        }
    }

    // Playlist management
    pub fn create_playlist(
        &self,
        _name: &str,
        _public: Option<bool>,
        _description: Option<&str>,
    ) -> Result<String, String> {
        info!("create playlist stubbed");
        Err("Playlist creation stubbed".to_string())
    }

    pub fn delete_playlist(&self, _id: &str) -> Result<(), String> {
        info!("delete playlist stubbed");
        Ok(())
    }

    pub fn overwrite_playlist(&self, _id: &str, _tracks: &[Playable]) {
        info!("overwrite playlist stubbed");
    }

    pub fn user_playlist_follow_playlist(&self, _id: &str) -> Result<(), String> {
        info!("follow playlist stubbed");
        Ok(())
    }

    pub fn user_playlist_unfollow_playlist(&self, _id: &str) -> bool {
        info!("unfollow playlist stubbed");
        true
    }

    pub fn user_playlist_add_tracks(
        &self,
        _playlist_id: &str,
        _track_ids: &[String],
        _position: Option<usize>,
    ) -> bool {
        info!("add tracks to playlist stubbed");
        true
    }

    pub fn user_playlist_remove_tracks(
        &self,
        _playlist_id: &str,
        _snapshot_id: Option<String>,
        _positions: &[usize],
    ) -> bool {
        info!("remove tracks from playlist stubbed");
        true
    }

    // Browse
    pub fn categories(&self) -> ApiResult<Category> {
        ApiResult::new(50, std::sync::Arc::new(|_offset| None))
    }

    pub fn category_playlists(&self, _category_id: &str, _offset: u32) -> ApiPage<Playlist> {
        ApiPage {
            offset: 0,
            total: 0,
            items: Vec::new(),
        }
    }

    // Recommendations (stub - recommendations are now handled via Library::get_radio_tracks)
    #[allow(dead_code)]
    pub fn recommendations(
        &self,
        _seed_tracks: Option<Vec<String>>,
        _seed_artists: Option<Vec<String>>,
    ) -> Vec<Track> {
        Vec::new()
    }
}
