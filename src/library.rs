use std::collections::HashMap;
use std::fs::File;
use std::iter::Iterator;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::thread;

use log::{debug, error, info};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::runtime::Runtime;

use crate::config::Config;
use crate::config::{self, CACHE_VERSION};
use crate::events::EventManager;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;

use crate::model::track::Track;
use crate::spotify::Spotify;
use crate::youtube_music::{
    YouTubeMusicClient,
    api::{
        LibraryAlbum, LibraryPlaylist, LibraryTrack, RadioTrack, SearchAlbum, SearchArtist,
        SearchPlaylist, SearchResults, SearchTrack, get_library_albums, get_library_playlists,
        get_liked_songs, get_radio, get_radio_continuation, search,
    },
};

/// Cached tracks database filename.
const CACHE_TRACKS: &str = "tracks.db";

/// Cached albums database filename.
const CACHE_ALBUMS: &str = "albums.db";

/// Cached artists database filename.
const CACHE_ARTISTS: &str = "artists.db";

/// Cached playlists database filename.
const CACHE_PLAYLISTS: &str = "playlists.db";

/// The user library with all their saved tracks, albums, playlists... High level interface to the
/// YouTube Music API used to manage items in the user library.
#[derive(Clone)]
pub struct Library {
    pub tracks: Arc<RwLock<Vec<Track>>>,
    pub albums: Arc<RwLock<Vec<Album>>>,
    pub artists: Arc<RwLock<Vec<Artist>>>,
    pub playlists: Arc<RwLock<Vec<Playlist>>>,
    pub is_done: Arc<RwLock<bool>>,
    pub user_id: Option<String>,
    pub display_name: Option<String>,
    ev: EventManager,
    spotify: Spotify,
    /// YouTube Music API client for fetching library data.
    yt_client: Option<YouTubeMusicClient>,
    pub cfg: Arc<Config>,
}

impl Library {
    /// Create an empty library for use in tests. No cache is loaded and no API calls are made.
    #[cfg(test)]
    pub fn new_for_test(ev: EventManager, spotify: Spotify, cfg: Arc<Config>) -> Arc<Self> {
        Arc::new(Self {
            tracks: Arc::new(RwLock::new(Vec::new())),
            albums: Arc::new(RwLock::new(Vec::new())),
            artists: Arc::new(RwLock::new(Vec::new())),
            playlists: Arc::new(RwLock::new(Vec::new())),
            is_done: Arc::new(RwLock::new(false)),
            user_id: None,
            display_name: None,
            ev,
            spotify,
            yt_client: None,
            cfg,
        })
    }

    /// Create a new library with YouTube Music client.
    pub fn new_with_client(
        ev: EventManager,
        spotify: Spotify,
        yt_client: YouTubeMusicClient,
        cfg: Arc<Config>,
    ) -> Self {
        let library = Self {
            tracks: Arc::new(RwLock::new(Vec::new())),
            albums: Arc::new(RwLock::new(Vec::new())),
            artists: Arc::new(RwLock::new(Vec::new())),
            playlists: Arc::new(RwLock::new(Vec::new())),
            is_done: Arc::new(RwLock::new(false)),
            user_id: None,
            display_name: None,
            ev,
            spotify,
            yt_client: Some(yt_client),
            cfg,
        };

        library.update_library();
        library
    }

    /// Load cached items from the file at `cache_path` into the given `store`.
    fn load_cache<T: DeserializeOwned>(&self, cache_path: &Path, store: &mut Vec<T>) {
        let saved_cache_version = self.cfg.state().cache_version;
        if saved_cache_version < CACHE_VERSION {
            debug!(
                "Cache version for {cache_path:?} has changed from {saved_cache_version} to {CACHE_VERSION}, ignoring cache"
            );
            return;
        }

        if let Ok(contents) = std::fs::read_to_string(cache_path) {
            debug!("loading cache from {}", cache_path.display());
            // Parse from in-memory string instead of directly from the file because it's faster.
            let parsed = serde_json::from_str::<Vec<_>>(&contents);
            match parsed {
                Ok(cache) => {
                    debug!(
                        "cache from {} loaded ({} items)",
                        cache_path.display(),
                        cache.len()
                    );
                    store.clear();
                    store.extend(cache);

                    // force refresh of UI (if visible)
                    self.trigger_redraw();
                }
                Err(e) => {
                    error!("can't parse cache: {e}");
                }
            }
        }
    }

    /// Save the items from `store` in the file at `cache_path`.
    fn save_cache<T: Serialize>(&self, cache_path: &Path, store: &[T]) {
        let cache_file = File::create(cache_path).unwrap();
        let serialize_result = serde_json::to_writer(cache_file, store);
        if let Err(message) = serialize_result {
            error!("could not write cache: {message:?}");
        }
    }

    /// Check whether the `remote` [Playlist] is newer than its locally saved version. Returns
    /// `true` if it is or if a local version isn't found.
    fn needs_download(&self, remote: &Playlist) -> bool {
        self.playlists
            .read()
            .unwrap()
            .iter()
            .find(|local| local.id == remote.id)
            .map(|local| local.num_tracks != remote.num_tracks)
            .unwrap_or(true)
    }

    /// Append `updated` to the local playlists or update the local version if it exists. Return the
    /// index of the appended/updated playlist.
    fn append_or_update(&self, updated: Playlist) -> usize {
        let mut store = self.playlists.write().unwrap();
        for (index, local) in store.iter_mut().enumerate() {
            if local.id == updated.id {
                *local = updated;
                return index;
            }
        }
        store.push(updated);
        store.len() - 1
    }

    /// Delete the playlist with the given `id` if it exists.
    pub fn delete_playlist(&self, id: &str) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        let position = self
            .playlists
            .read()
            .unwrap()
            .iter()
            .position(|i| i.id == id);

        if let Some(position) = position
            && self.spotify.api.delete_playlist(id).is_ok()
        {
            self.playlists.write().unwrap().remove(position);
            self.save_cache(
                &config::cache_path(CACHE_PLAYLISTS),
                &self.playlists.read().unwrap(),
            );
        }
    }

    /// Set the playlist with `id` to contain only `tracks`. If the playlist already contains
    /// tracks, they will be removed. Update the cache to match the new state.
    pub fn overwrite_playlist(&self, id: &str, tracks: &[Playable]) {
        debug!("saving {} tracks to list {}", tracks.len(), id);
        self.spotify.api.overwrite_playlist(id, tracks);

        self.fetch_playlists();
        self.save_cache(
            &config::cache_path(CACHE_PLAYLISTS),
            &self.playlists.read().unwrap(),
        );
    }

    /// Create a playlist with the given `name` and add `tracks` to it.
    pub fn save_playlist(&self, name: &str, tracks: &[Playable]) {
        debug!("saving {} tracks to new list {}", tracks.len(), name);
        match self.spotify.api.create_playlist(name, None, None) {
            Ok(id) => self.overwrite_playlist(&id, tracks),
            Err(_) => error!("could not create new playlist.."),
        }
    }

    /// Update the local library and its cache on disk.
    pub fn update_library(&self) {
        *self.is_done.write().unwrap() = false;

        let library = self.clone();
        thread::spawn(move || {
            let t_tracks = {
                let library = library.clone();
                thread::spawn(move || {
                    library.load_cache(
                        &config::cache_path(CACHE_TRACKS),
                        library.tracks.write().unwrap().as_mut(),
                    );
                    library.fetch_tracks();
                    library.save_cache(
                        &config::cache_path(CACHE_TRACKS),
                        &library.tracks.read().unwrap(),
                    );
                })
            };

            let t_albums = {
                let library = library.clone();
                thread::spawn(move || {
                    library.load_cache(
                        &config::cache_path(CACHE_ALBUMS),
                        library.albums.write().unwrap().as_mut(),
                    );
                    library.fetch_albums();
                    library.save_cache(
                        &config::cache_path(CACHE_ALBUMS),
                        &library.albums.read().unwrap(),
                    );
                })
            };

            let t_artists = {
                let library = library.clone();
                thread::spawn(move || {
                    library.load_cache(
                        &config::cache_path(CACHE_ARTISTS),
                        library.artists.write().unwrap().as_mut(),
                    );
                    library.fetch_artists();
                })
            };

            let t_playlists = {
                let library = library.clone();
                thread::spawn(move || {
                    library.load_cache(
                        &config::cache_path(CACHE_PLAYLISTS),
                        library.playlists.write().unwrap().as_mut(),
                    );
                    library.fetch_playlists();
                    library.save_cache(
                        &config::cache_path(CACHE_PLAYLISTS),
                        &library.playlists.read().unwrap(),
                    );
                })
            };

            t_tracks.join().unwrap();
            t_artists.join().unwrap();

            library.populate_artists();
            library.save_cache(
                &config::cache_path(CACHE_ARTISTS),
                &library.artists.read().unwrap(),
            );

            t_albums.join().unwrap();
            t_playlists.join().unwrap();

            let mut is_done = library.is_done.write().unwrap();
            *is_done = true;

            library.ev.trigger();
        });
    }

    /// Fetch the playlists from YouTube Music and save them to the local library. This synchronizes
    /// the local version with the remote, pruning removed playlists in the process.
    fn fetch_playlists(&self) {
        debug!("loading playlists");

        // Use YouTube Music client if available
        if let Some(ref client) = self.yt_client {
            let runtime = match Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    error!("Failed to create runtime for fetching playlists: {}", e);
                    return;
                }
            };

            let mut stale_lists = self.playlists.read().unwrap().clone();
            let mut list_order = Vec::new();
            let mut continuation: Option<String> = None;
            let mut page_num = 0u32;

            loop {
                debug!("playlists page: {}", page_num);
                page_num += 1;

                let result =
                    runtime.block_on(get_library_playlists(client, continuation.as_deref()));

                match result {
                    Ok(response) => {
                        for lib_playlist in response.items.iter() {
                            let playlist = Self::library_playlist_to_playlist(lib_playlist);
                            list_order.push(playlist.id.clone());

                            // Remove from stale playlists so we won't prune it later
                            if let Some(index) =
                                stale_lists.iter().position(|x| x.id == playlist.id)
                            {
                                stale_lists.remove(index);
                            }

                            // Update or add the playlist
                            self.append_or_update(playlist);
                        }

                        // Check for more pages
                        if let Some(token) = response.continuation {
                            continuation = Some(token);
                        } else {
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Failed to fetch playlists: {:?}", e);
                        break;
                    }
                }
            }

            // Remove stale playlists
            for stale in stale_lists {
                let index = self
                    .playlists
                    .read()
                    .unwrap()
                    .iter()
                    .position(|x| x.id == stale.id);
                if let Some(index) = index {
                    debug!("removing stale list: {:?}", stale.name);
                    self.playlists.write().unwrap().remove(index);
                }
            }

            // Sort by remote order
            self.playlists.write().unwrap().sort_by(|a, b| {
                let a_index = list_order.iter().position(|x| x == &a.id);
                let b_index = list_order.iter().position(|x| x == &b.id);
                a_index.cmp(&b_index)
            });

            info!("Loaded {} playlists", self.playlists.read().unwrap().len());
            self.trigger_redraw();
        } else {
            // Fallback to stub API (returns empty)
            debug!("No YouTube Music client, using stub API");
            let mut stale_lists = self.playlists.read().unwrap().clone();
            let mut list_order = Vec::new();

            let lists_page = self.spotify.api.current_user_playlist();
            let mut lists_batch = Some(lists_page.items.read().unwrap().clone());
            while let Some(lists) = lists_batch {
                for (index, remote) in lists.iter().enumerate() {
                    list_order.push(remote.id.clone());

                    // remove from stale playlists so we won't prune it later on
                    if let Some(index) = stale_lists.iter().position(|x| x.id == remote.id) {
                        stale_lists.remove(index);
                    }

                    if self.needs_download(remote) {
                        info!("updating playlist {} (index: {})", remote.name, index);
                        let mut playlist: Playlist = remote.clone();
                        playlist.tracks = None;
                        playlist.load_tracks(&self.spotify);
                        self.append_or_update(playlist);
                        // trigger redraw
                        self.trigger_redraw();
                    }
                }
                lists_batch = lists_page.next();
            }

            // remove stale playlists
            for stale in stale_lists {
                let index = self
                    .playlists
                    .read()
                    .unwrap()
                    .iter()
                    .position(|x| x.id == stale.id);
                if let Some(index) = index {
                    debug!("removing stale list: {:?}", stale.name);
                    self.playlists.write().unwrap().remove(index);
                }
            }

            // sort by remote order
            self.playlists.write().unwrap().sort_by(|a, b| {
                let a_index = list_order.iter().position(|x| x == &a.id);
                let b_index = list_order.iter().position(|x| x == &b.id);
                a_index.cmp(&b_index)
            });

            // trigger redraw
            self.trigger_redraw();
        }
    }

    /// Convert a LibraryPlaylist from the API to our Playlist model.
    fn library_playlist_to_playlist(lib_playlist: &LibraryPlaylist) -> Playlist {
        // Parse track count from string like "50 songs"
        let num_tracks = lib_playlist
            .track_count
            .as_ref()
            .and_then(|s| s.split_whitespace().next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);

        Playlist {
            id: lib_playlist.playlist_id.clone(),
            name: lib_playlist.title.clone(),
            owner_id: String::new(), // YouTube Music doesn't expose owner ID in library response
            owner_name: None,
            num_tracks,
            tracks: None,
            thumbnail_url: lib_playlist.thumbnail_url.clone(),
            description: None,
        }
    }

    /// Fetch the artists from the web API and save them to the local library.
    fn fetch_artists(&self) {
        let mut artists: Vec<Artist> = Vec::new();
        let mut last: Option<String> = None;
        let mut i = 0u32;

        loop {
            let page = self
                .spotify
                .api
                .current_user_followed_artists(last.as_deref());
            debug!("artists page: {i}");
            i += 1;
            if page.is_err() {
                error!("Failed to fetch artists.");
                return;
            }
            let page = page.unwrap();

            artists.extend(page.items.clone());

            if page.next.is_some() {
                last = artists.last().and_then(|a| a.id.clone());
            } else {
                break;
            }
        }

        let mut store = self.artists.write().unwrap();

        for mut artist in artists {
            let pos = store.iter().position(|a| a.id == artist.id);
            if let Some(i) = pos {
                store[i].is_followed = true;
                continue;
            }

            artist.is_followed = true;

            store.push(artist);
        }
    }

    /// Add the artist with `id` and `name` to the user library, but don't sync with the API.
    /// This does not add if there is already an artist with `id`.
    fn insert_artist(&self, id: String, name: String) {
        let mut artists = self.artists.write().unwrap();

        if !artists
            .iter()
            .any(|a| a.id.as_ref().is_some_and(|value| *value == id))
        {
            let mut artist = Artist::new(id.to_string(), name.to_string());
            artist.tracks = Some(Vec::new());
            artists.push(artist);
        }
    }

    /// Fetch the albums from YouTube Music and store them in the local library.
    fn fetch_albums(&self) {
        debug!("loading albums");

        // Use YouTube Music client if available
        if let Some(ref client) = self.yt_client {
            let runtime = match Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    error!("Failed to create runtime for fetching albums: {}", e);
                    return;
                }
            };

            let mut albums: Vec<Album> = Vec::new();
            let mut continuation: Option<String> = None;
            let mut page_num = 0u32;

            loop {
                debug!("albums page: {}", page_num);
                page_num += 1;

                let result = runtime.block_on(get_library_albums(client, continuation.as_deref()));

                match result {
                    Ok(response) => {
                        // Convert LibraryAlbum to Album
                        for lib_album in response.items.iter() {
                            albums.push(Self::library_album_to_album(lib_album));
                        }

                        // Check for more pages
                        if let Some(token) = response.continuation {
                            continuation = Some(token);
                        } else {
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Failed to fetch albums: {:?}", e);
                        break;
                    }
                }
            }

            // Sort albums by artist, year, title
            albums.sort_unstable_by_key(|album| {
                let album_artist = album
                    .artists
                    .first()
                    .map(|a| a.strip_prefix("The ").unwrap_or(a))
                    .unwrap_or("");
                let album_title = album.title.strip_prefix("The ").unwrap_or(&album.title);
                format!(
                    "{}{}{}",
                    album_artist.to_lowercase(),
                    album.year,
                    album_title.to_lowercase()
                )
            });

            info!("Loaded {} albums", albums.len());
            *self.albums.write().unwrap() = albums;
        } else {
            // Fallback to stub API (returns empty)
            debug!("No YouTube Music client, using stub API");
            let mut albums: Vec<Album> = Vec::new();
            let mut i = 0u32;

            loop {
                let page = self
                    .spotify
                    .api
                    .current_user_saved_albums(albums.len() as u32);
                debug!("albums page: {i}");

                i += 1;

                if page.is_err() {
                    error!("Failed to fetch albums.");
                    return;
                }

                let page = page.unwrap();
                albums.extend(page.items.iter().map(|a| a.into()));

                if page.next.is_none() {
                    break;
                }
            }

            albums.sort_unstable_by_key(|album| {
                let album_artist = album.artists[0]
                    .strip_prefix("The ")
                    .unwrap_or(&album.artists[0]);
                let album_title = album.title.strip_prefix("The ").unwrap_or(&album.title);
                format!(
                    "{}{}{}",
                    album_artist.to_lowercase(),
                    album.year,
                    album_title.to_lowercase()
                )
            });

            *self.albums.write().unwrap() = albums;
        }
    }

    /// Convert a LibraryAlbum from the API to our Album model.
    fn library_album_to_album(lib_album: &LibraryAlbum) -> Album {
        Album {
            id: Some(lib_album.browse_id.clone()),
            title: lib_album.title.clone(),
            artists: lib_album.artists.iter().map(|a| a.name.clone()).collect(),
            artist_ids: lib_album
                .artists
                .iter()
                .filter_map(|a| a.browse_id.clone())
                .collect(),
            year: lib_album.year.clone().unwrap_or_default(),
            cover_url: lib_album.thumbnail_url.clone(),
            tracks: None,
            added_at: None,
            audio_playlist_id: None,
            is_explicit: lib_album.is_explicit,
        }
    }

    /// Fetch the tracks (liked songs) from YouTube Music and save them in the local library.
    fn fetch_tracks(&self) {
        debug!("loading tracks (liked songs)");

        // Use YouTube Music client if available
        if let Some(ref client) = self.yt_client {
            let runtime = match Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    error!("Failed to create runtime for fetching tracks: {}", e);
                    return;
                }
            };

            let mut tracks = Vec::new();
            let mut continuation: Option<String> = None;
            let mut page_num = 0u32;

            loop {
                debug!("tracks page: {}", page_num);
                page_num += 1;

                let result = runtime.block_on(get_liked_songs(client, continuation.as_deref()));

                match result {
                    Ok(response) => {
                        // Convert LibraryTrack to Track
                        for (index, lib_track) in response.items.iter().enumerate() {
                            tracks.push(Self::library_track_to_track(
                                lib_track,
                                tracks.len() + index,
                            ));
                        }

                        // Check for more pages
                        if let Some(token) = response.continuation {
                            continuation = Some(token);
                        } else {
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Failed to fetch tracks: {:?}", e);
                        break;
                    }
                }
            }

            info!("Loaded {} liked songs", tracks.len());
            *self.tracks.write().unwrap() = tracks;
        } else {
            // Fallback to stub API (returns empty)
            debug!("No YouTube Music client, using stub API");
            let mut tracks = Vec::new();
            let mut i = 0u32;

            loop {
                let page = self
                    .spotify
                    .api
                    .current_user_saved_tracks(tracks.len() as u32);

                debug!("tracks page: {i}");
                i += 1;

                if page.is_err() {
                    error!("Failed to fetch tracks.");
                    return;
                }
                let page = page.unwrap();

                tracks.extend(page.items.iter().map(|t| t.into()));

                if page.next.is_none() {
                    break;
                }
            }

            *self.tracks.write().unwrap() = tracks;
        }
    }

    /// Convert a LibraryTrack from the API to our Track model.
    fn library_track_to_track(lib_track: &LibraryTrack, index: usize) -> Track {
        Track {
            id: Some(lib_track.video_id.clone()),
            title: lib_track.title.clone(),
            duration: lib_track.duration_seconds.unwrap_or(0),
            artists: lib_track.artists.iter().map(|a| a.name.clone()).collect(),
            artist_ids: lib_track
                .artists
                .iter()
                .filter_map(|a| a.browse_id.clone())
                .collect(),
            album: lib_track.album.as_ref().map(|a| a.title.clone()),
            album_id: lib_track.album.as_ref().and_then(|a| a.browse_id.clone()),
            cover_url: lib_track.thumbnail_url.clone(),
            added_at: None,
            list_index: index,
            is_explicit: lib_track.is_explicit,
            set_video_id: lib_track.set_video_id.clone(),
        }
    }

    /// Convert a RadioTrack from the API to our Track model.
    fn radio_track_to_track(radio_track: &RadioTrack, index: usize) -> Track {
        Track {
            id: Some(radio_track.video_id.clone()),
            title: radio_track.title.clone(),
            duration: radio_track.duration_seconds.unwrap_or(0),
            artists: radio_track.artists.iter().map(|a| a.name.clone()).collect(),
            artist_ids: radio_track
                .artists
                .iter()
                .filter_map(|a| a.browse_id.clone())
                .collect(),
            album: radio_track.album.as_ref().map(|a| a.title.clone()),
            album_id: radio_track.album.as_ref().and_then(|a| a.browse_id.clone()),
            cover_url: radio_track.thumbnail_url.clone(),
            added_at: None,
            list_index: index,
            is_explicit: radio_track.is_explicit,
            set_video_id: None,
        }
    }

    /// Get radio (similar tracks) based on a video ID.
    ///
    /// This fetches tracks from YouTube Music's radio/automix feature,
    /// which generates a playlist of similar tracks based on a seed track.
    ///
    /// # Arguments
    ///
    /// * `video_id` - The YouTube video ID to base the radio on
    ///
    /// # Returns
    ///
    /// A vector of similar tracks, or an empty vector if the feature is unavailable.
    pub fn get_radio_tracks(&self, video_id: &str) -> Vec<Track> {
        // Target number of radio tracks to fetch
        const TARGET_TRACKS: usize = 100;

        if let Some(ref client) = self.yt_client {
            let runtime = match Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    error!("Failed to create runtime for fetching radio: {}", e);
                    return Vec::new();
                }
            };

            // Fetch initial radio response
            let initial = match runtime.block_on(get_radio(client, video_id)) {
                Ok(response) => response,
                Err(e) => {
                    error!("Failed to fetch radio tracks: {:?}", e);
                    return Vec::new();
                }
            };

            let mut all_tracks = initial.tracks;
            let mut continuation = initial.continuation;
            let playlist_id = initial.playlist_id;

            // Follow continuation tokens until we have enough tracks
            while all_tracks.len() < TARGET_TRACKS {
                let (Some(token), Some(pid)) = (continuation, &playlist_id) else {
                    break;
                };

                match runtime.block_on(get_radio_continuation(client, pid, &token)) {
                    Ok(response) => {
                        if response.tracks.is_empty() {
                            break;
                        }
                        all_tracks.extend(response.tracks);
                        continuation = response.continuation;
                    }
                    Err(e) => {
                        error!("Failed to fetch radio continuation: {:?}", e);
                        break;
                    }
                }
            }

            info!("Loaded {} radio tracks for {}", all_tracks.len(), video_id);
            all_tracks
                .iter()
                .enumerate()
                .map(|(index, rt)| Self::radio_track_to_track(rt, index))
                .collect()
        } else {
            debug!("No YouTube Music client available for radio");
            Vec::new()
        }
    }

    /// Search YouTube Music for tracks, albums, artists, and playlists.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query string
    ///
    /// # Returns
    ///
    /// Search results containing tracks, albums, artists, and playlists.
    pub fn search(&self, query: &str) -> SearchResults {
        if let Some(ref client) = self.yt_client {
            let runtime = match Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    error!("Failed to create runtime for search: {}", e);
                    return SearchResults::default();
                }
            };

            match runtime.block_on(search(client, query)) {
                Ok(results) => {
                    info!(
                        "Search results for '{}': {} tracks, {} albums, {} artists, {} playlists",
                        query,
                        results.tracks.len(),
                        results.albums.len(),
                        results.artists.len(),
                        results.playlists.len()
                    );
                    results
                }
                Err(e) => {
                    error!("Failed to search: {:?}", e);
                    SearchResults::default()
                }
            }
        } else {
            debug!("No YouTube Music client available for search");
            SearchResults::default()
        }
    }

    /// Convert a SearchTrack from the API to our Track model.
    pub fn search_track_to_track(search_track: &SearchTrack, index: usize) -> Track {
        Track {
            id: Some(search_track.video_id.clone()),
            title: search_track.title.clone(),
            duration: search_track.duration_seconds.unwrap_or(0),
            artists: search_track
                .artists
                .iter()
                .map(|a| a.name.clone())
                .collect(),
            artist_ids: search_track
                .artists
                .iter()
                .filter_map(|a| a.browse_id.clone())
                .collect(),
            album: search_track.album.as_ref().map(|a| a.title.clone()),
            album_id: search_track
                .album
                .as_ref()
                .and_then(|a| a.browse_id.clone()),
            cover_url: search_track.thumbnail_url.clone(),
            added_at: None,
            list_index: index,
            is_explicit: search_track.is_explicit,
            set_video_id: None,
        }
    }

    /// Convert a SearchAlbum from the API to our Album model.
    pub fn search_album_to_album(search_album: &SearchAlbum) -> Album {
        Album {
            id: Some(search_album.browse_id.clone()),
            title: search_album.title.clone(),
            artists: search_album
                .artists
                .iter()
                .map(|a| a.name.clone())
                .collect(),
            artist_ids: search_album
                .artists
                .iter()
                .filter_map(|a| a.browse_id.clone())
                .collect(),
            year: search_album.year.clone().unwrap_or_default(),
            cover_url: search_album.thumbnail_url.clone(),
            tracks: None,
            added_at: None,
            audio_playlist_id: None,
            is_explicit: search_album.is_explicit,
        }
    }

    /// Convert a SearchArtist from the API to our Artist model.
    pub fn search_artist_to_artist(search_artist: &SearchArtist) -> Artist {
        Artist {
            id: Some(search_artist.browse_id.clone()),
            name: search_artist.name.clone(),
            thumbnail_url: search_artist.thumbnail_url.clone(),
            tracks: None,
            is_followed: false,
            subscribers: search_artist.subscribers.clone(),
        }
    }

    /// Convert a SearchPlaylist from the API to our Playlist model.
    pub fn search_playlist_to_playlist(search_playlist: &SearchPlaylist) -> Playlist {
        Playlist {
            id: search_playlist.browse_id.clone(),
            name: search_playlist.title.clone(),
            owner_id: String::new(),
            owner_name: search_playlist.author.clone(),
            num_tracks: 0,
            tracks: None,
            thumbnail_url: search_playlist.thumbnail_url.clone(),
            description: None,
        }
    }

    fn populate_artists(&self) {
        // Remove old unfollowed artists
        {
            let mut artists = self.artists.write().unwrap();
            *artists = artists.iter().filter(|a| a.is_followed).cloned().collect();
        }

        // Add artists that aren't followed but have saved tracks
        {
            let tracks = self.tracks.read().unwrap();
            let mut track_artists: Vec<(&String, &String)> = tracks
                .iter()
                .flat_map(|t| t.artist_ids.iter().zip(t.artists.iter()))
                .collect();
            track_artists.dedup_by(|a, b| a.0 == b.0);

            for (id, name) in track_artists.iter() {
                self.insert_artist(id.to_string(), name.to_string());
            }
        }

        let mut artists = self.artists.write().unwrap();
        let mut lookup: HashMap<String, Option<usize>> = HashMap::new();

        // Make sure only saved tracks are played when playing artists
        for artist in artists.iter_mut() {
            artist.tracks = Some(Vec::new());
        }

        artists.sort_unstable_by(|a, b| {
            let a_cmp = a.name.strip_prefix("The ").unwrap_or(&a.name);
            let b_cmp = b.name.strip_prefix("The ").unwrap_or(&b.name);

            a_cmp.partial_cmp(b_cmp).unwrap()
        });

        // Add saved tracks to artists
        {
            let tracks = self.tracks.read().unwrap();
            for track in tracks.iter() {
                for artist_id in &track.artist_ids {
                    let index = if let Some(i) = lookup.get(artist_id).cloned() {
                        i
                    } else {
                        let i = artists
                            .iter()
                            .position(|a| &a.id.clone().unwrap_or_default() == artist_id);
                        lookup.insert(artist_id.clone(), i);
                        i
                    };

                    if let Some(i) = index {
                        let artist = artists.get_mut(i).unwrap();
                        if artist.tracks.is_none() {
                            artist.tracks = Some(Vec::new());
                        }

                        if let Some(tracks) = artist.tracks.as_mut() {
                            if tracks.iter().any(|t| t.id == track.id) {
                                continue;
                            }

                            tracks.push(track.clone());
                        }
                    }
                }
            }
        }
    }

    /// Check whether `track` is saved in the user's library.
    pub fn is_saved_track(&self, track: &Playable) -> bool {
        if !*self.is_done.read().unwrap() {
            return false;
        }

        let tracks = self.tracks.read().unwrap();
        tracks.iter().any(|t| t.id == track.id())
    }

    /// Save `tracks` to the user's library.
    pub fn save_tracks(&self, tracks: &[&Track]) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        let save_tracks_result = self
            .spotify
            .api
            .current_user_saved_tracks_add(tracks.iter().filter_map(|t| t.id.as_deref()).collect());

        if save_tracks_result.is_err() {
            return;
        }

        {
            let mut store = self.tracks.write().unwrap();
            let mut i = 0;
            for track in tracks {
                if store.iter().any(|t| t.id == track.id) {
                    continue;
                }

                store.insert(i, (*track).clone());
                i += 1;
            }
        }

        self.populate_artists();

        self.save_cache(
            &config::cache_path(CACHE_TRACKS),
            &self.tracks.read().unwrap(),
        );
        self.save_cache(
            &config::cache_path(CACHE_ARTISTS),
            &self.artists.read().unwrap(),
        );
    }

    /// Remove `tracks` from the user's library.
    pub fn unsave_tracks(&self, tracks: &[&Track]) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        if self
            .spotify
            .api
            .current_user_saved_tracks_delete(
                tracks.iter().filter_map(|t| t.id.as_deref()).collect(),
            )
            .is_err()
        {
            return;
        }

        {
            let mut store = self.tracks.write().unwrap();
            *store = store
                .iter()
                .filter(|t| !tracks.iter().any(|tt| t.id == tt.id))
                .cloned()
                .collect();
        }

        self.populate_artists();

        self.save_cache(
            &config::cache_path(CACHE_TRACKS),
            &self.tracks.read().unwrap(),
        );
        self.save_cache(
            &config::cache_path(CACHE_ARTISTS),
            &self.artists.read().unwrap(),
        );
    }

    /// Check whether `album` is saved to the user's library.
    pub fn is_saved_album(&self, album: &Album) -> bool {
        if !*self.is_done.read().unwrap() {
            return false;
        }

        let albums = self.albums.read().unwrap();
        albums.iter().any(|a| a.id == album.id)
    }

    /// Save `album` to the user's library.
    pub fn save_album(&self, album: &Album) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        if let Some(ref album_id) = album.id
            && self
                .spotify
                .api
                .current_user_saved_albums_add(vec![album_id.as_str()])
                .is_err()
        {
            return;
        }

        {
            let mut store = self.albums.write().unwrap();
            if !store.iter().any(|a| a.id == album.id) {
                store.insert(0, album.clone());

                // resort list of albums
                store.sort_unstable_by_key(|a| format!("{}{}{}", a.artists[0], a.year, a.title));
            }
        }

        self.save_cache(
            &config::cache_path(CACHE_ALBUMS),
            &self.albums.read().unwrap(),
        );
    }

    /// Remove `album` from the user's library.
    pub fn unsave_album(&self, album: &Album) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        if let Some(ref album_id) = album.id
            && self
                .spotify
                .api
                .current_user_saved_albums_delete(vec![album_id.as_str()])
                .is_err()
        {
            return;
        }

        {
            let mut store = self.albums.write().unwrap();
            *store = store.iter().filter(|a| a.id != album.id).cloned().collect();
        }

        self.save_cache(
            &config::cache_path(CACHE_ALBUMS),
            &self.albums.read().unwrap(),
        );
    }

    /// Check whether the user follows `artist`.
    pub fn is_followed_artist(&self, artist: &Artist) -> bool {
        if !*self.is_done.read().unwrap() {
            return false;
        }

        let artists = self.artists.read().unwrap();
        artists.iter().any(|a| a.id == artist.id && a.is_followed)
    }

    /// Follow `artist` as the logged in user.
    pub fn follow_artist(&self, artist: &Artist) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        if let Some(ref artist_id) = artist.id
            && self
                .spotify
                .api
                .user_follow_artists(vec![artist_id.as_str()])
                .is_err()
        {
            return;
        }

        {
            let mut store = self.artists.write().unwrap();
            if let Some(i) = store.iter().position(|a| a.id == artist.id) {
                store[i].is_followed = true;
            } else {
                let mut artist = artist.clone();
                artist.is_followed = true;
                store.push(artist);
            }
        }

        self.populate_artists();

        self.save_cache(
            &config::cache_path(CACHE_ARTISTS),
            &self.artists.read().unwrap(),
        );
    }

    /// Unfollow `artist` as the logged in user.
    pub fn unfollow_artist(&self, artist: &Artist) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        if let Some(ref artist_id) = artist.id
            && self
                .spotify
                .api
                .user_unfollow_artists(vec![artist_id.as_str()])
                .is_err()
        {
            return;
        }

        {
            let mut store = self.artists.write().unwrap();
            if let Some(i) = store.iter().position(|a| a.id == artist.id) {
                store[i].is_followed = false;
            }
        }

        self.populate_artists();

        self.save_cache(
            &config::cache_path(CACHE_ARTISTS),
            &self.artists.read().unwrap(),
        );
    }

    /// Check whether `playlist` is in the library but not created by the library's owner.
    pub fn is_followed_playlist(&self, playlist: &Playlist) -> bool {
        self.user_id
            .as_ref()
            .map(|id| id != &playlist.owner_id)
            .unwrap_or(false)
    }

    /// Add `playlist` to the user's library by following it as the logged in user.
    pub fn follow_playlist(&self, mut playlist: Playlist) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        let follow_playlist_result = self.spotify.api.user_playlist_follow_playlist(&playlist.id);

        if follow_playlist_result.is_err() {
            return;
        }

        playlist.load_tracks(&self.spotify);

        {
            let mut store = self.playlists.write().unwrap();
            if !store.iter().any(|p| p.id == playlist.id) {
                store.insert(0, playlist);
            }
        }

        self.save_cache(
            &config::cache_path(CACHE_PLAYLISTS),
            &self.playlists.read().unwrap(),
        );
    }

    /// Remove a playlist from the user's library by unfollowing it.
    pub fn unfollow_playlist(&self, playlist_id: &str) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        if !self
            .spotify
            .api
            .user_playlist_unfollow_playlist(playlist_id)
        {
            return;
        }

        {
            let mut store = self.playlists.write().unwrap();
            store.retain(|p| p.id != playlist_id);
        }

        self.save_cache(
            &config::cache_path(CACHE_PLAYLISTS),
            &self.playlists.read().unwrap(),
        );
    }

    /// Force redraw the user interface.
    pub fn trigger_redraw(&self) {
        self.ev.trigger();
    }
}
