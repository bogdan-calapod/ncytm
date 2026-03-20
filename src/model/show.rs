use crate::library::Library;
use crate::model::episode::Episode;
use crate::model::playable::Playable;
use crate::queue::Queue;
use crate::spotify::Spotify;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::show::ShowView;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct Show {
    pub id: String,
    pub uri: String,
    pub name: String,
    pub publisher: String,
    pub description: String,
    pub cover_url: Option<String>,
    pub episodes: Option<Vec<Episode>>,
}

impl Show {
    pub fn load_all_episodes(&mut self, spotify: Spotify) {
        if self.episodes.is_some() {
            return;
        }
        let page = spotify.api.show_episodes(&self.id, 0);
        self.episodes = Some(page.items);
    }
}

impl fmt::Display for Show {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl ListItem for Show {
    fn is_playing(&self, _queue: &Queue) -> bool {
        false
    }

    fn display_left(&self, _library: &Library) -> String {
        format!("{self}")
    }

    fn display_right(&self, library: &Library) -> String {
        if library.is_followed_show(self) {
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
        self.load_all_episodes(queue.get_spotify());

        if let Some(episodes) = self.episodes.as_ref() {
            let tracks: Vec<Playable> = episodes
                .iter()
                .map(|ep| Playable::Episode(ep.clone()))
                .collect();
            let index = queue.append_next(&tracks);
            queue.play(index, true, false);
        }
    }

    fn play_next(&mut self, queue: &Queue) {
        self.load_all_episodes(queue.get_spotify());

        if let Some(episodes) = self.episodes.as_ref() {
            for ep in episodes.iter().rev() {
                queue.insert_after_current(Playable::Episode(ep.clone()));
            }
        }
    }

    fn queue(&mut self, queue: &Queue) {
        self.load_all_episodes(queue.get_spotify());

        if let Some(episodes) = self.episodes.as_ref() {
            for ep in episodes {
                queue.append(Playable::Episode(ep.clone()));
            }
        }
    }

    fn toggle_saved(&mut self, library: &Library) {
        if library.is_followed_show(self) {
            library.unfollow_show(self);
        } else {
            library.follow_show(self);
        }
    }

    fn save(&mut self, library: &Library) {
        library.follow_show(self);
    }

    fn unsave(&mut self, library: &Library) {
        library.unfollow_show(self);
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        Some(ShowView::new(queue, library, self).into_boxed_view_ext())
    }

    fn share_url(&self) -> Option<String> {
        Some(format!("https://music.youtube.com/channel/{}", self.id))
    }

    #[inline]
    fn is_saved(&self, library: &Library) -> Option<bool> {
        Some(library.is_followed_show(self))
    }

    #[inline]
    fn is_playable(&self) -> bool {
        true
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
