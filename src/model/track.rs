use std::fmt;
use std::sync::{Arc, RwLock};

use crate::config;
use crate::utils::ms_to_hms;
use chrono::{DateTime, Utc};

use crate::library::Library;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::playable::Playable;
use crate::queue::Queue;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::listview::ListView;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Track {
    pub id: Option<String>,
    pub uri: String,
    pub title: String,
    pub track_number: u32,
    pub disc_number: i32,
    pub duration: u32,
    pub artists: Vec<String>,
    pub artist_ids: Vec<String>,
    pub album: Option<String>,
    pub album_id: Option<String>,
    pub album_artists: Vec<String>,
    pub cover_url: Option<String>,
    pub url: String,
    pub added_at: Option<DateTime<Utc>>,
    pub list_index: usize,
    pub is_local: bool,
    pub is_playable: Option<bool>,
}

impl Track {
    pub fn new(id: Option<String>, title: String, artists: Vec<String>, duration: u32) -> Self {
        let uri = id
            .as_ref()
            .map(|i| format!("youtube:track:{}", i))
            .unwrap_or_default();
        let url = id
            .as_ref()
            .map(|i| format!("https://music.youtube.com/watch?v={}", i))
            .unwrap_or_default();
        Self {
            id,
            uri,
            title,
            track_number: 0,
            disc_number: 0,
            duration,
            artists,
            artist_ids: Vec::new(),
            album: None,
            album_id: None,
            album_artists: Vec::new(),
            cover_url: None,
            url,
            added_at: None,
            list_index: 0,
            is_local: false,
            is_playable: Some(true),
        }
    }

    pub fn duration_str(&self) -> String {
        ms_to_hms(self.duration)
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

        let recommendations: Vec<Track> = if let Some(id) = &self.id {
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
        self.is_playable == Some(true)
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
