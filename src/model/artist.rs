use std::fmt;
use std::sync::Arc;

use crate::library::Library;
use crate::model::playable::Playable;
use crate::model::track::Track;
use crate::queue::Queue;
use crate::spotify::Spotify;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::artist::ArtistView;

#[derive(Clone, Deserialize, Serialize, Debug, Default)]
pub struct Artist {
    pub id: Option<String>,
    pub name: String,
    pub url: Option<String>,
    pub tracks: Option<Vec<Track>>,
    pub is_followed: bool,
}

impl Artist {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id: Some(id),
            name,
            url: None,
            tracks: None,
            is_followed: false,
        }
    }

    fn load_top_tracks(&mut self, spotify: Spotify) {
        if let Some(artist_id) = &self.id {
            if self.tracks.is_none() {
                self.tracks = Some(spotify.api.artist_top_tracks(artist_id));
            }
        }
    }
}

impl fmt::Display for Artist {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl ListItem for Artist {
    fn is_playing(&self, _queue: &Queue) -> bool {
        false
    }

    fn display_left(&self, _library: &Library) -> String {
        format!("{self}")
    }

    fn display_right(&self, library: &Library) -> String {
        if library.is_followed_artist(self) {
            if library.cfg.values().use_nerdfont.unwrap_or(false) {
                "\u{f012c}".to_string()
            } else {
                "✓".to_string()
            }
        } else {
            String::new()
        }
    }

    fn play(&mut self, queue: &Queue) {
        self.load_top_tracks(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            let tracks: Vec<Playable> = tracks
                .iter()
                .map(|track| Playable::Track(track.clone()))
                .collect();
            let index = queue.append_next(&tracks);
            queue.play(index, true, false);
        }
    }

    fn play_next(&mut self, queue: &Queue) {
        self.load_top_tracks(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            for t in tracks.iter().rev() {
                queue.insert_after_current(Playable::Track(t.clone()));
            }
        }
    }

    fn queue(&mut self, queue: &Queue) {
        self.load_top_tracks(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            for t in tracks {
                queue.append(Playable::Track(t.clone()));
            }
        }
    }

    fn toggle_saved(&mut self, library: &Library) {
        if library.is_followed_artist(self) {
            library.unfollow_artist(self);
        } else {
            library.follow_artist(self);
        }
    }

    fn save(&mut self, library: &Library) {
        library.follow_artist(self);
    }

    fn unsave(&mut self, library: &Library) {
        library.unfollow_artist(self);
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        Some(ArtistView::new(queue, library, self).into_boxed_view_ext())
    }

    fn share_url(&self) -> Option<String> {
        self.id
            .as_ref()
            .map(|id| format!("https://music.youtube.com/channel/{}", id))
    }

    #[inline]
    fn is_saved(&self, library: &Library) -> Option<bool> {
        Some(library.is_followed_artist(self))
    }

    #[inline]
    fn is_playable(&self) -> bool {
        true
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
