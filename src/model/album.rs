use rand::{rng, seq::IteratorRandom};
use std::fmt;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};

use crate::library::Library;
use crate::model::artist::Artist;
use crate::model::playable::Playable;
use crate::model::track::Track;
use crate::queue::Queue;
use crate::spotify::Spotify;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::{album::AlbumView, listview::ListView};

/// An album from YouTube Music.
#[derive(Clone, Deserialize, Serialize)]
pub struct Album {
    /// Album browse ID (primary identifier).
    pub id: Option<String>,
    /// Album title.
    pub title: String,
    /// Artist names.
    pub artists: Vec<String>,
    /// Artist browse IDs (channel IDs).
    pub artist_ids: Vec<String>,
    /// Release year.
    pub year: String,
    /// Thumbnail/cover URL.
    pub cover_url: Option<String>,
    /// Tracks in this album.
    pub tracks: Option<Vec<Track>>,
    /// When the album was added to library (if applicable).
    pub added_at: Option<DateTime<Utc>>,
    /// Audio playlist ID (for playback).
    pub audio_playlist_id: Option<String>,
    /// Whether the album contains explicit content.
    pub is_explicit: bool,
}

impl Album {
    /// Load all tracks for this album from the API.
    pub fn load_all_tracks(&mut self, spotify: Spotify) {
        // Skip if tracks are already loaded
        if self.tracks.is_some() {
            return;
        }

        if let Some(ref album_id) = self.id
            && let Ok(full_album) = spotify.api.album(album_id)
        {
            self.tracks = full_album.tracks.clone();
        }
    }
}

impl fmt::Display for Album {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} - {}", self.artists.join(", "), self.title)
    }
}

impl fmt::Debug for Album {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "({} - {} ({:?}))",
            self.artists.join(", "),
            self.title,
            self.id
        )
    }
}

impl ListItem for Album {
    fn is_playing(&self, queue: &Queue) -> bool {
        if let Some(tracks) = self.tracks.as_ref() {
            let playing: Vec<String> = queue
                .queue
                .read()
                .unwrap()
                .iter()
                .filter_map(|t| t.id())
                .collect();

            let ids: Vec<String> = tracks.iter().filter_map(|t| t.id.clone()).collect();
            !ids.is_empty() && playing == ids
        } else {
            false
        }
    }

    fn display_left(&self, _library: &Library) -> String {
        format!("{self}")
    }

    fn display_right(&self, library: &Library) -> String {
        let saved = if library.is_saved_album(self) {
            if library.cfg.values().use_nerdfont.unwrap_or(false) {
                "\u{f012c} "
            } else {
                "✓ "
            }
        } else {
            ""
        };
        format!("{}{}", saved, self.year)
    }

    fn play(&mut self, queue: &Queue) {
        self.load_all_tracks(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            let tracks: Vec<Playable> = tracks
                .iter()
                .map(|track| Playable::Track(track.clone()))
                .collect();
            let index = queue.append_next(&tracks);
            queue.play(index, true, true);
        }
    }

    fn play_next(&mut self, queue: &Queue) {
        self.load_all_tracks(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            for t in tracks.iter().rev() {
                queue.insert_after_current(Playable::Track(t.clone()));
            }
        }
    }

    fn queue(&mut self, queue: &Queue) {
        self.load_all_tracks(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            for t in tracks {
                queue.append(Playable::Track(t.clone()));
            }
        }
    }

    fn toggle_saved(&mut self, library: &Library) {
        if library.is_saved_album(self) {
            library.unsave_album(self);
        } else {
            library.save_album(self);
        }
    }

    fn save(&mut self, library: &Library) {
        library.save_album(self);
    }

    fn unsave(&mut self, library: &Library) {
        library.unsave_album(self);
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        Some(AlbumView::new(queue, library, self).into_boxed_view_ext())
    }

    fn open_recommendations(
        &mut self,
        queue: Arc<Queue>,
        library: Arc<Library>,
    ) -> Option<Box<dyn ViewExt>> {
        self.load_all_tracks(queue.get_spotify());
        let track_ids: Vec<String> = self
            .tracks
            .as_ref()?
            .iter()
            .filter_map(|t| t.id.clone())
            .take(4)
            .collect();

        let artist_id: Option<String> = self.artist_ids.iter().cloned().choose(&mut rng());

        if track_ids.is_empty() && artist_id.is_none() {
            return None;
        }

        let spotify = queue.get_spotify();
        let recommendations = spotify
            .api
            .recommendations(Some(track_ids), artist_id.map(|a| vec![a]));

        if recommendations.is_empty() {
            None
        } else {
            Some(
                ListView::new(
                    Arc::new(RwLock::new(recommendations)),
                    queue.clone(),
                    library.clone(),
                )
                .with_title(&format!("Similar to Album \"{}\"", self.title))
                .into_boxed_view_ext(),
            )
        }
    }

    fn share_url(&self) -> Option<String> {
        self.id
            .clone()
            .map(|id| format!("https://music.youtube.com/playlist?list={id}"))
    }

    fn artists(&self) -> Option<Vec<Artist>> {
        Some(
            self.artist_ids
                .iter()
                .zip(self.artists.iter())
                .map(|(id, name)| Artist::new(id.clone(), name.clone()))
                .collect(),
        )
    }

    #[inline]
    fn is_saved(&self, library: &Library) -> Option<bool> {
        Some(library.is_saved_album(self))
    }

    #[inline]
    fn is_playable(&self) -> bool {
        true
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
