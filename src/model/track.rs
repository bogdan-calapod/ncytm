use std::fmt;
use std::sync::{Arc, RwLock};

use crate::config;
use crate::utils::seconds_to_hms;
use chrono::{DateTime, Utc};

use crate::library::Library;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::playable::Playable;
use crate::queue::Queue;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::listview::ListView;

/// A playable track (song) from YouTube Music.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Track {
    /// YouTube video ID (primary identifier).
    pub id: Option<String>,
    /// Track title.
    pub title: String,
    /// Duration in seconds.
    pub duration: u32,
    /// Artist names.
    pub artists: Vec<String>,
    /// Artist browse IDs (channel IDs).
    pub artist_ids: Vec<String>,
    /// Album title.
    pub album: Option<String>,
    /// Album browse ID.
    pub album_id: Option<String>,
    /// Thumbnail/cover URL.
    pub cover_url: Option<String>,
    /// When the track was added to library (if applicable).
    pub added_at: Option<DateTime<Utc>>,
    /// Index in a list (for UI purposes).
    pub list_index: usize,
    /// Whether the track contains explicit content.
    pub is_explicit: bool,
    /// Set video ID (for removing from liked songs).
    pub set_video_id: Option<String>,
}

impl Track {
    /// Format the duration as a human-readable string (e.g., "3:45").
    pub fn duration_str(&self) -> String {
        seconds_to_hms(self.duration)
    }
}

impl fmt::Display for Track {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} - {}", self.artists.join(", "), self.title)
    }
}

impl ListItem for Track {
    fn is_playing(&self, queue: &Queue) -> bool {
        let current = queue.get_current();
        current.map(|t| t.id() == self.id).unwrap_or(false)
    }

    fn display_left(&self, library: &Library) -> String {
        let formatting = library
            .cfg
            .values()
            .track_format
            .clone()
            .unwrap_or_default();
        let default = config::TrackFormat::default().left.unwrap();
        let left = formatting.left.unwrap_or_else(|| default.clone());
        if left != default {
            Playable::format(&Playable::Track(self.clone()), &left, library)
        } else {
            format!("{self}")
        }
    }

    fn display_center(&self, library: &Library) -> String {
        let formatting = library
            .cfg
            .values()
            .track_format
            .clone()
            .unwrap_or_default();
        let default = config::TrackFormat::default().center.unwrap();
        let center = formatting.center.unwrap_or_else(|| default.clone());
        if center != default {
            Playable::format(&Playable::Track(self.clone()), &center, library)
        } else {
            self.album.clone().unwrap_or_default()
        }
    }

    fn display_right(&self, library: &Library) -> String {
        let formatting = library
            .cfg
            .values()
            .track_format
            .clone()
            .unwrap_or_default();
        let default = config::TrackFormat::default().right.unwrap();
        let right = formatting.right.unwrap_or_else(|| default.clone());
        if right != default {
            Playable::format(&Playable::Track(self.clone()), &right, library)
        } else {
            let saved = if library.is_saved_track(&Playable::Track(self.clone())) {
                if library.cfg.values().use_nerdfont.unwrap_or(false) {
                    "\u{f012c}"
                } else {
                    "✓"
                }
            } else {
                ""
            };
            format!("{} {}", saved, self.duration_str())
        }
    }

    fn play(&mut self, queue: &Queue) {
        let index = queue.append_next(&vec![Playable::Track(self.clone())]);
        queue.play(index, true, false);
    }

    fn play_next(&mut self, queue: &Queue) {
        queue.insert_after_current(Playable::Track(self.clone()));
    }

    fn queue(&mut self, queue: &Queue) {
        queue.append(Playable::Track(self.clone()));
    }

    fn toggle_saved(&mut self, library: &Library) {
        if library.is_saved_track(&Playable::Track(self.clone())) {
            library.unsave_tracks(&[self]);
        } else {
            library.save_tracks(&[self]);
        }
    }

    fn save(&mut self, library: &Library) {
        library.save_tracks(&[self]);
    }

    fn unsave(&mut self, library: &Library) {
        library.unsave_tracks(&[self]);
    }

    fn open(&self, _queue: Arc<Queue>, _library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        None
    }

    fn open_recommendations(
        &mut self,
        queue: Arc<Queue>,
        library: Arc<Library>,
    ) -> Option<Box<dyn ViewExt>> {
        let spotify = queue.get_spotify();

        let recommendations: Vec<Self> = if let Some(id) = &self.id {
            spotify.api.recommendations(Some(vec![id.clone()]), None)
        } else {
            Vec::new()
        };

        if recommendations.is_empty() {
            None
        } else {
            Some(
                ListView::new(
                    Arc::new(RwLock::new(recommendations)),
                    queue.clone(),
                    library.clone(),
                )
                .with_title(&format!(
                    "Similar to \"{} - {}\"",
                    self.artists.join(", "),
                    self.title
                ))
                .into_boxed_view_ext(),
            )
        }
    }

    fn share_url(&self) -> Option<String> {
        self.id
            .clone()
            .map(|id| format!("https://music.youtube.com/watch?v={id}"))
    }

    fn album(&self, queue: &Queue) -> Option<Album> {
        let spotify = queue.get_spotify();

        match self.album_id {
            Some(ref album_id) => spotify.api.album(album_id).ok(),
            None => None,
        }
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

    fn track(&self) -> Option<Track> {
        Some(self.clone())
    }

    #[inline]
    fn is_saved(&self, library: &Library) -> Option<bool> {
        Some(library.is_saved_track(&Playable::Track(self.clone())))
    }

    #[inline]
    fn is_playable(&self) -> bool {
        // All tracks from YouTube Music are playable
        true
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
