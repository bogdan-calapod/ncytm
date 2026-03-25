use crate::library::Library;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::track::Track;
use crate::queue::Queue;
use crate::traits::{ListItem, ViewExt};
use crate::utils::seconds_to_hms;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum Playable {
    Track(Track),
}

impl Playable {
    pub fn format(playable: &Self, formatting: &str, library: &Library) -> String {
        formatting
            .replace(
                "%artists",
                if let Some(artists) = playable.artists() {
                    artists
                        .iter()
                        .map(|artist| artist.clone().name)
                        .collect::<Vec<String>>()
                        .join(", ")
                } else {
                    String::new()
                }
                .as_str(),
            )
            .replace(
                "%artist",
                if let Some(artists) = playable.artists() {
                    artists
                        .first()
                        .map_or(String::from(""), |artist| artist.name.clone())
                } else {
                    String::new()
                }
                .as_str(),
            )
            .replace(
                "%title",
                match playable.clone() {
                    Self::Track(track) => track.title,
                }
                .as_str(),
            )
            .replace(
                "%album",
                match playable.clone() {
                    Self::Track(track) => track.album.unwrap_or_default(),
                }
                .as_str(),
            )
            .replace(
                "%saved",
                if library.is_saved_track(playable) {
                    if library.cfg.values().use_nerdfont.unwrap_or_default() {
                        "\u{f012c}"
                    } else {
                        "✓"
                    }
                } else {
                    ""
                },
            )
            .replace("%duration", playable.duration_str().as_str())
    }

    pub fn id(&self) -> Option<String> {
        match self {
            Self::Track(track) => track.id.clone(),
        }
    }

    pub fn cover_url(&self) -> Option<String> {
        match self {
            Self::Track(track) => track.cover_url.clone(),
        }
    }

    pub fn duration(&self) -> u32 {
        match self {
            Self::Track(track) => track.duration,
        }
    }

    pub fn duration_str(&self) -> String {
        seconds_to_hms(self.duration())
    }

    pub fn title(&self) -> String {
        match self {
            Self::Track(track) => track.title.clone(),
        }
    }

    pub fn as_listitem(&self) -> Box<dyn ListItem> {
        match self {
            Self::Track(track) => track.as_listitem(),
        }
    }
}

impl fmt::Display for Playable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Track(track) => track.fmt(f),
        }
    }
}

impl ListItem for Playable {
    fn is_playable(&self) -> bool {
        self.as_listitem().is_playable()
    }

    fn is_playing(&self, queue: &Queue) -> bool {
        self.as_listitem().is_playing(queue)
    }

    fn display_left(&self, library: &Library) -> String {
        self.as_listitem().display_left(library)
    }

    fn display_center(&self, library: &Library) -> String {
        self.as_listitem().display_center(library)
    }

    fn display_right(&self, library: &Library) -> String {
        self.as_listitem().display_right(library)
    }

    fn play(&mut self, queue: &Queue) {
        self.as_listitem().play(queue)
    }

    fn play_next(&mut self, queue: &Queue) {
        self.as_listitem().play_next(queue)
    }

    fn queue(&mut self, queue: &Queue) {
        self.as_listitem().queue(queue)
    }

    fn toggle_saved(&mut self, library: &Library) {
        self.as_listitem().toggle_saved(library)
    }

    fn save(&mut self, library: &Library) {
        self.as_listitem().save(library)
    }

    fn unsave(&mut self, library: &Library) {
        self.as_listitem().unsave(library)
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        self.as_listitem().open(queue, library)
    }

    fn share_url(&self) -> Option<String> {
        self.as_listitem().share_url()
    }

    fn album(&self, queue: &Queue) -> Option<Album> {
        self.as_listitem().album(queue)
    }

    fn artists(&self) -> Option<Vec<Artist>> {
        self.as_listitem().artists()
    }

    fn track(&self) -> Option<Track> {
        self.as_listitem().track()
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        self.as_listitem()
    }
}
