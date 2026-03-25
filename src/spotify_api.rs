//! Web API functionality.
//! Provides integration between the UI layer and YouTube Music API.

use std::sync::Arc;

use log::info;

use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::category::Category;
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::model::track::Track;
use crate::ui::pagination::{ApiPage, ApiResult};
use crate::youtube_music::{Cookies, YouTubeMusicClient, api as yt_api};

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

/// Web API wrapper for YouTube Music.
#[derive(Clone)]
pub struct WebApi {
    /// YouTube Music API client (optional, lazy-initialized).
    client: Option<Arc<YouTubeMusicClient>>,
}

impl Default for WebApi {
    fn default() -> Self {
        Self::new()
    }
}

impl WebApi {
    pub fn new() -> Self {
        Self { client: None }
    }

    /// Create a new WebApi with a YouTube Music client.
    #[allow(dead_code)]
    pub fn with_client(client: YouTubeMusicClient) -> Self {
        Self {
            client: Some(Arc::new(client)),
        }
    }

    /// Set the YouTube Music client.
    #[allow(dead_code)]
    pub fn set_client(&mut self, client: YouTubeMusicClient) {
        self.client = Some(Arc::new(client));
    }

    /// Initialize the client from cookies if not already set.
    pub fn init_from_cookies(&mut self, cookies: Cookies) -> Result<(), String> {
        if self.client.is_none() {
            let client = YouTubeMusicClient::new(cookies)
                .map_err(|e| format!("Failed to create client: {:?}", e))?;
            self.client = Some(Arc::new(client));
        }
        Ok(())
    }

    /// Get a reference to the client, if available.
    fn get_client(&self) -> Option<&YouTubeMusicClient> {
        self.client.as_ref().map(|c| c.as_ref())
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

    pub fn user_follow_artists(&self, ids: Vec<&str>) -> Result<(), String> {
        let client = self.get_client().ok_or("No client available")?;
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        for id in ids {
            let result = rt.block_on(async {
                // Subscribe to artist channel via the subscribe endpoint
                let body = serde_json::json!({
                    "channelIds": [id]
                });
                client.post("subscription/subscribe", &body).await
            });
            if let Err(e) = result {
                return Err(e.to_string());
            }
        }
        Ok(())
    }

    pub fn user_unfollow_artists(&self, ids: Vec<&str>) -> Result<(), String> {
        let client = self.get_client().ok_or("No client available")?;
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        for id in ids {
            let result = rt.block_on(async {
                let body = serde_json::json!({
                    "channelIds": [id]
                });
                client.post("subscription/unsubscribe", &body).await
            });
            if let Err(e) = result {
                return Err(e.to_string());
            }
        }
        Ok(())
    }

    // Content retrieval
    pub fn track(&self, id: &str) -> Option<Track> {
        // Fetch a single track by video ID using a search query.
        // YouTube Music has no direct "get video metadata" endpoint accessible
        // without a session, so we use the player endpoint.
        let client = self.get_client()?;
        let rt = tokio::runtime::Runtime::new().ok()?;
        let result = rt.block_on(async { yt_api::search(client, id).await });
        match result {
            Ok(results) => results
                .tracks
                .into_iter()
                .find(|t| t.video_id == id)
                .map(|t| Track {
                    id: Some(t.video_id),
                    title: t.title,
                    duration: t.duration_seconds.unwrap_or(0),
                    artists: t.artists.iter().map(|a| a.name.clone()).collect(),
                    artist_ids: t
                        .artists
                        .iter()
                        .filter_map(|a| a.browse_id.clone())
                        .collect(),
                    album: t.album.as_ref().map(|a| a.title.clone()),
                    album_id: t.album.and_then(|a| a.browse_id),
                    cover_url: t.thumbnail_url,
                    added_at: None,
                    list_index: 0,
                    is_explicit: t.is_explicit,
                    set_video_id: None,
                }),
            Err(_) => None,
        }
    }

    pub fn album(&self, id: &str) -> Result<Album, String> {
        let client = self.get_client().ok_or("No client available")?;

        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        let result = rt.block_on(async { yt_api::get_album(client, id).await });

        match result {
            Ok(page) => {
                let details = page.details.ok_or("No album details found")?;

                // Convert tracks
                let tracks: Vec<Track> = page
                    .tracks
                    .into_iter()
                    .map(|t| album_track_to_track(t, &details))
                    .collect();

                Ok(Album {
                    id: Some(details.browse_id),
                    title: details.title,
                    artists: details.artists.iter().map(|a| a.name.clone()).collect(),
                    artist_ids: details
                        .artists
                        .iter()
                        .filter_map(|a| a.browse_id.clone())
                        .collect(),
                    year: details.year.unwrap_or_default(),
                    cover_url: details.thumbnail_url,
                    tracks: Some(tracks),
                    added_at: None,
                    audio_playlist_id: details.audio_playlist_id,
                    is_explicit: details.is_explicit,
                })
            }
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn artist(&self, id: &str) -> Option<Artist> {
        let client = self.get_client()?;

        let rt = tokio::runtime::Runtime::new().ok()?;
        let result = rt.block_on(async { yt_api::get_artist(client, id).await });

        match result {
            Ok(page) => {
                let details = page.details?;
                Some(Artist {
                    id: Some(details.browse_id),
                    name: details.name,
                    thumbnail_url: details.thumbnail_url,
                    tracks: Some(
                        page.top_tracks
                            .into_iter()
                            .map(artist_track_to_track)
                            .collect(),
                    ),
                    is_followed: false,
                    subscribers: details.subscribers,
                })
            }
            Err(_) => None,
        }
    }

    pub fn artist_albums(&self, id: &str, _limit: u32, _offset: u32) -> ApiPage<Album> {
        let Some(client) = self.get_client() else {
            return ApiPage {
                offset: 0,
                total: 0,
                items: Vec::new(),
            };
        };

        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(_) => {
                return ApiPage {
                    offset: 0,
                    total: 0,
                    items: Vec::new(),
                };
            }
        };

        let result = rt.block_on(async { yt_api::get_artist(client, id).await });

        match result {
            Ok(page) => {
                // Combine albums and singles
                let mut albums: Vec<Album> = page
                    .albums
                    .into_iter()
                    .map(|a| artist_album_to_album(a, &page.details))
                    .collect();
                let singles: Vec<Album> = page
                    .singles
                    .into_iter()
                    .map(|a| artist_album_to_album(a, &page.details))
                    .collect();
                albums.extend(singles);

                let total = albums.len() as u32;
                ApiPage {
                    offset: 0,
                    total,
                    items: albums,
                }
            }
            Err(_) => ApiPage {
                offset: 0,
                total: 0,
                items: Vec::new(),
            },
        }
    }

    pub fn artist_top_tracks(&self, id: &str) -> Vec<Track> {
        let Some(client) = self.get_client() else {
            return Vec::new();
        };

        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(_) => return Vec::new(),
        };

        let result = rt.block_on(async { yt_api::get_artist(client, id).await });

        match result {
            Ok(page) => page
                .top_tracks
                .into_iter()
                .map(artist_track_to_track)
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn artist_related_artists(&self, id: &str) -> Vec<Artist> {
        let Some(client) = self.get_client() else {
            return Vec::new();
        };

        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(_) => return Vec::new(),
        };

        let result = rt.block_on(async { yt_api::get_artist(client, id).await });

        match result {
            Ok(page) => page
                .related_artists
                .into_iter()
                .map(|a| Artist {
                    id: Some(a.browse_id),
                    name: a.name,
                    thumbnail_url: a.thumbnail_url,
                    tracks: None,
                    is_followed: false,
                    subscribers: a.subscribers,
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn playlist(&self, id: &str) -> Option<Playlist> {
        let client = self.get_client()?;
        let rt = tokio::runtime::Runtime::new().ok()?;
        let result = rt.block_on(async { yt_api::get_playlist_info(client, id).await });
        match result {
            Ok(Some(info)) => Some(Playlist {
                id: info.playlist_id,
                name: info.title,
                owner_id: info.owner_id,
                owner_name: info.owner_name,
                num_tracks: info.track_count,
                tracks: None,
                thumbnail_url: info.thumbnail_url,
                description: info.description,
            }),
            _ => None,
        }
    }

    pub fn playlist_tracks(
        &self,
        id: &str,
        _limit: u32,
        _offset: u32,
    ) -> Option<ApiPage<Playable>> {
        let client = self.get_client()?;
        let rt = tokio::runtime::Runtime::new().ok()?;

        // Fetch all pages
        let mut all_tracks: Vec<Playable> = Vec::new();
        let mut continuation: Option<String> = None;

        loop {
            let cont_ref = continuation.as_deref();
            let result =
                rt.block_on(async { yt_api::get_playlist_tracks(client, id, cont_ref).await });
            match result {
                Ok(page) => {
                    for t in page.tracks {
                        all_tracks.push(Playable::Track(playlist_track_to_track(t)));
                    }
                    continuation = page.continuation;
                    if continuation.is_none() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        let total = all_tracks.len() as u32;
        Some(ApiPage {
            offset: 0,
            total,
            items: all_tracks,
        })
    }

    // Playlist management
    pub fn create_playlist(
        &self,
        name: &str,
        public: Option<bool>,
        description: Option<&str>,
    ) -> Result<String, String> {
        let client = self.get_client().ok_or("No client available")?;
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(async {
            yt_api::create_playlist(client, name, description, public.unwrap_or(false)).await
        })
        .map_err(|e| e.to_string())
    }

    pub fn delete_playlist(&self, id: &str) -> Result<(), String> {
        let client = self.get_client().ok_or("No client available")?;
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(async { yt_api::delete_playlist(client, id).await })
            .map_err(|e| e.to_string())
    }

    pub fn overwrite_playlist(&self, id: &str, tracks: &[Playable]) {
        let Some(client) = self.get_client() else {
            return;
        };
        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return;
        };

        // 1. Delete all existing tracks by first fetching them to get set_video_ids
        let existing = rt.block_on(async { yt_api::get_playlist_tracks(client, id, None).await });
        if let Ok(page) = existing {
            let svids: Vec<&str> = page
                .tracks
                .iter()
                .filter_map(|t| t.set_video_id.as_deref())
                .collect();
            let vids: Vec<&str> = page.tracks.iter().map(|t| t.video_id.as_str()).collect();
            if !svids.is_empty() {
                let _ = rt.block_on(async {
                    yt_api::remove_playlist_tracks(client, id, &svids, &vids).await
                });
            }
        }

        // 2. Add new tracks
        let owned_ids: Vec<String> = tracks.iter().filter_map(|t| t.id()).collect();
        let id_refs: Vec<&str> = owned_ids.iter().map(|s| s.as_str()).collect();
        if !id_refs.is_empty() {
            let _ = rt.block_on(async { yt_api::add_playlist_tracks(client, id, &id_refs).await });
        }
    }

    pub fn user_playlist_follow_playlist(&self, id: &str) -> Result<(), String> {
        let client = self.get_client().ok_or("No client available")?;
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(async { yt_api::follow_playlist(client, id).await })
            .map_err(|e| e.to_string())
    }

    pub fn user_playlist_unfollow_playlist(&self, id: &str) -> bool {
        let Some(client) = self.get_client() else {
            return false;
        };
        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return false;
        };
        rt.block_on(async { yt_api::unfollow_playlist(client, id).await })
            .is_ok()
    }

    pub fn user_playlist_add_tracks(
        &self,
        playlist_id: &str,
        track_ids: &[String],
        _position: Option<usize>,
    ) -> bool {
        let Some(client) = self.get_client() else {
            return false;
        };
        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return false;
        };
        let id_refs: Vec<&str> = track_ids.iter().map(|s| s.as_str()).collect();
        rt.block_on(async { yt_api::add_playlist_tracks(client, playlist_id, &id_refs).await })
            .is_ok()
    }

    pub fn user_playlist_remove_tracks(
        &self,
        playlist_id: &str,
        _snapshot_id: Option<String>,
        positions: &[usize],
    ) -> bool {
        // To remove by position we first need to fetch the tracks to get their set_video_ids.
        let Some(client) = self.get_client() else {
            return false;
        };
        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return false;
        };
        let page_result =
            rt.block_on(async { yt_api::get_playlist_tracks(client, playlist_id, None).await });
        let page = match page_result {
            Ok(p) => p,
            Err(_) => return false,
        };

        let to_remove: Vec<&yt_api::PlaylistTrack> = positions
            .iter()
            .filter_map(|&i| page.tracks.get(i))
            .collect();

        let svids: Vec<&str> = to_remove
            .iter()
            .filter_map(|t| t.set_video_id.as_deref())
            .collect();
        let vids: Vec<&str> = to_remove.iter().map(|t| t.video_id.as_str()).collect();

        if svids.is_empty() {
            return true; // nothing to remove
        }

        rt.block_on(async {
            yt_api::remove_playlist_tracks(client, playlist_id, &svids, &vids).await
        })
        .is_ok()
    }

    // Browse
    pub fn categories(&self) -> ApiResult<Category> {
        let Some(client) = self.get_client() else {
            return ApiResult::new(50, std::sync::Arc::new(|_offset| None));
        };
        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return ApiResult::new(50, std::sync::Arc::new(|_offset| None));
        };

        let cats = rt
            .block_on(async { yt_api::get_categories(client).await })
            .unwrap_or_default();

        let categories: Vec<Category> = cats
            .into_iter()
            .map(|c| Category {
                id: c.browse_id,
                name: c.name,
            })
            .collect();

        let total = categories.len() as u32;
        let items = Arc::new(std::sync::RwLock::new(categories));
        ApiResult::new(
            total,
            Arc::new(move |_offset| {
                Some(ApiPage {
                    offset: 0,
                    total,
                    items: items.read().unwrap().clone(),
                })
            }),
        )
    }

    pub fn category_playlists(&self, category_id: &str, _offset: u32) -> ApiPage<Playlist> {
        let Some(client) = self.get_client() else {
            return ApiPage {
                offset: 0,
                total: 0,
                items: Vec::new(),
            };
        };
        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return ApiPage {
                offset: 0,
                total: 0,
                items: Vec::new(),
            };
        };

        let pls = rt
            .block_on(async { yt_api::get_category_playlists(client, category_id).await })
            .unwrap_or_default();

        let playlists: Vec<Playlist> = pls
            .into_iter()
            .map(|p| Playlist {
                id: p.playlist_id,
                name: p.title,
                owner_id: String::new(),
                owner_name: p.subtitle,
                num_tracks: 0,
                tracks: None,
                thumbnail_url: p.thumbnail_url,
                description: None,
            })
            .collect();

        let total = playlists.len() as u32;
        ApiPage {
            offset: 0,
            total,
            items: playlists,
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

/// Convert an AlbumTrack from the API to a Track model.
fn album_track_to_track(track: yt_api::AlbumTrack, album_details: &yt_api::AlbumDetails) -> Track {
    Track {
        id: Some(track.video_id),
        title: track.title,
        duration: track.duration_seconds.unwrap_or(0),
        artists: track.artists.iter().map(|a| a.name.clone()).collect(),
        artist_ids: track
            .artists
            .iter()
            .filter_map(|a| a.browse_id.clone())
            .collect(),
        album: Some(album_details.title.clone()),
        album_id: Some(album_details.browse_id.clone()),
        cover_url: track
            .thumbnail_url
            .or_else(|| album_details.thumbnail_url.clone()),
        added_at: None,
        list_index: track.track_number.unwrap_or(0) as usize,
        is_explicit: track.is_explicit,
        set_video_id: None,
    }
}

/// Convert a PlaylistTrack from the API to a Track model.
fn playlist_track_to_track(track: yt_api::PlaylistTrack) -> Track {
    Track {
        id: Some(track.video_id),
        title: track.title,
        duration: track.duration_seconds.unwrap_or(0),
        artists: track.artists.iter().map(|a| a.name.clone()).collect(),
        artist_ids: track
            .artists
            .iter()
            .filter_map(|a| a.browse_id.clone())
            .collect(),
        album: track.album,
        album_id: track.album_id,
        cover_url: track.thumbnail_url,
        added_at: None,
        list_index: 0,
        is_explicit: track.is_explicit,
        set_video_id: track.set_video_id,
    }
}

/// Convert an ArtistTrack from the API to a Track model.
fn artist_track_to_track(track: yt_api::ArtistTrack) -> Track {
    Track {
        id: Some(track.video_id),
        title: track.title,
        duration: track.duration_seconds.unwrap_or(0),
        artists: track.artists.iter().map(|a| a.name.clone()).collect(),
        artist_ids: track
            .artists
            .iter()
            .filter_map(|a| a.browse_id.clone())
            .collect(),
        album: track.album.as_ref().map(|a| a.title.clone()),
        album_id: track.album.and_then(|a| a.browse_id),
        cover_url: track.thumbnail_url,
        added_at: None,
        list_index: 0,
        is_explicit: track.is_explicit,
        set_video_id: None,
    }
}

/// Convert an ArtistAlbum from the API to an Album model.
fn artist_album_to_album(
    album: yt_api::ArtistAlbum,
    artist_details: &Option<yt_api::ArtistDetails>,
) -> Album {
    // Get artist name from the artist details if available
    let (artists, artist_ids) = if let Some(details) = artist_details {
        (vec![details.name.clone()], vec![details.browse_id.clone()])
    } else {
        (Vec::new(), Vec::new())
    };

    Album {
        id: Some(album.browse_id),
        title: album.title,
        artists,
        artist_ids,
        year: album.year.unwrap_or_default(),
        cover_url: album.thumbnail_url,
        tracks: None,
        added_at: None,
        audio_playlist_id: None,
        is_explicit: album.is_explicit,
    }
}
