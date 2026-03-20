use std::iter::Iterator;
use std::sync::{Arc, RwLock};

use log::debug;

use crate::model::playable::Playable;
use crate::queue::Queue;
use crate::spotify::Spotify;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::{listview::ListView, playlist::PlaylistView};
use crate::{command::SortDirection, command::SortKey, library::Library};

/// A playlist from YouTube Music.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Playlist {
    /// Playlist ID (primary identifier).
    pub id: String,
    /// Playlist name/title.
    pub name: String,
    /// Owner/author channel ID.
    pub owner_id: String,
    /// Owner/author display name.
    pub owner_name: Option<String>,
    /// Number of tracks in the playlist.
    pub num_tracks: usize,
    /// Tracks in this playlist.
    pub tracks: Option<Vec<Playable>>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Playlist description.
    pub description: Option<String>,
}

impl Playlist {
    /// Load tracks for this playlist from the API.
    pub fn load_tracks(&mut self, spotify: &Spotify) {
        if self.tracks.is_some() {
            return;
        }
        // Fetch tracks from API
        if let Some(page) = spotify.api.playlist_tracks(&self.id, 50, 0) {
            self.tracks = Some(page.items);
            self.num_tracks = page.total as usize;
        }
    }

    /// Check if this playlist contains a specific track.
    pub fn has_track(&self, track_id: &str) -> bool {
        self.tracks
            .as_ref()
            .map(|tracks| {
                tracks
                    .iter()
                    .any(|t| t.id().map(|id| id == track_id).unwrap_or(false))
            })
            .unwrap_or(false)
    }

    /// Delete a track from this playlist at the given index.
    pub fn delete_track(&mut self, index: usize, spotify: &Spotify) {
        if let Some(ref mut tracks) = self.tracks {
            spotify
                .api
                .user_playlist_remove_tracks(&self.id, None, &[index]);
            tracks.remove(index);
        }
    }

    /// Append tracks to this playlist.
    pub fn append_tracks<'a, I: Iterator<Item = &'a Playable>>(
        &mut self,
        new_tracks: I,
        spotify: &Spotify,
    ) {
        let track_ids: Vec<String> = new_tracks.filter_map(|t| t.id()).collect();
        if track_ids.is_empty() {
            return;
        }
        spotify
            .api
            .user_playlist_add_tracks(&self.id, &track_ids, None);
        // Reload tracks
        self.tracks = None;
        self.load_tracks(spotify);
    }

    /// Sort tracks in this playlist.
    pub fn sort(&mut self, key: &SortKey, direction: &SortDirection, _spotify: &Spotify) {
        debug!("Sorting playlist by {:?} {:?}", key, direction);
        // TODO: Implement playlist sorting
    }
}

impl std::fmt::Display for Playlist {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl ListItem for Playlist {
    fn is_playing(&self, queue: &Queue) -> bool {
        if let Some(tracks) = self.tracks.as_ref() {
            let playing: Vec<String> = queue
                .queue
                .read()
                .unwrap()
                .iter()
                .filter_map(|t| t.id())
                .collect();

            let ids: Vec<String> = tracks.iter().filter_map(|t| t.id()).collect();
            !ids.is_empty() && playing == ids
        } else {
            false
        }
    }

    fn display_left(&self, _library: &Library) -> String {
        format!("{self}")
    }

    fn display_right(&self, library: &Library) -> String {
        let followed = library.is_followed_playlist(self);
        let icon = if followed {
            if library.cfg.values().use_nerdfont.unwrap_or(false) {
                "\u{f012c} "
            } else {
                "✓ "
            }
        } else {
            ""
        };
        format!("{}{} tracks", icon, self.num_tracks)
    }

    fn play(&mut self, queue: &Queue) {
        self.load_tracks(&queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            let index = queue.append_next(tracks);
            queue.play(index, true, true);
        }
    }

    fn play_next(&mut self, queue: &Queue) {
        self.load_tracks(&queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            for t in tracks.iter().rev() {
                queue.insert_after_current(t.clone());
            }
        }
    }

    fn queue(&mut self, queue: &Queue) {
        self.load_tracks(&queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            for t in tracks {
                queue.append(t.clone());
            }
        }
    }

    fn toggle_saved(&mut self, library: &Library) {
        if library.is_followed_playlist(self) {
            library.unfollow_playlist(&self.id);
        } else {
            library.follow_playlist(self.clone());
        }
    }

    fn save(&mut self, library: &Library) {
        library.follow_playlist(self.clone());
    }

    fn unsave(&mut self, library: &Library) {
        library.unfollow_playlist(&self.id);
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        Some(PlaylistView::new(queue, library, self).into_boxed_view_ext())
    }

    fn open_recommendations(
        &mut self,
        queue: Arc<Queue>,
        library: Arc<Library>,
    ) -> Option<Box<dyn ViewExt>> {
        self.load_tracks(&queue.get_spotify());

        let track_ids: Vec<String> = self
            .tracks
            .as_ref()?
            .iter()
            .filter_map(|p| p.id())
            .take(5)
            .collect();

        if track_ids.is_empty() {
            return None;
        }

        let spotify = queue.get_spotify();
        let recommendations = spotify.api.recommendations(Some(track_ids), None);

        if recommendations.is_empty() {
            None
        } else {
            Some(
                ListView::new(
                    Arc::new(RwLock::new(recommendations)),
                    queue.clone(),
                    library.clone(),
                )
                .with_title(&format!("Similar to Playlist \"{}\"", self.name))
                .into_boxed_view_ext(),
            )
        }
    }

    fn share_url(&self) -> Option<String> {
        Some(format!(
            "https://music.youtube.com/playlist?list={}",
            self.id
        ))
    }

    #[inline]
    fn is_saved(&self, library: &Library) -> Option<bool> {
        Some(library.is_followed_playlist(self))
    }

    #[inline]
    fn is_playable(&self) -> bool {
        true
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
