use crate::library::Library;
use crate::model::playable::Playable;
use crate::queue::Queue;
use crate::traits::{ListItem, ViewExt};
use crate::utils::ms_to_hms;
use chrono::{DateTime, Utc};
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Episode {
    pub id: String,
    pub uri: String,
    pub duration: u32,
    pub name: String,
    pub description: String,
    pub release_date: String,
    pub cover_url: Option<String>,
    pub added_at: Option<DateTime<Utc>>,
    pub list_index: usize,
}

impl Episode {
    pub fn new(id: String, name: String, duration: u32) -> Self {
        Self {
            id: id.clone(),
            uri: format!("youtube:episode:{}", id),
            duration,
            name,
            description: String::new(),
            release_date: String::new(),
            cover_url: None,
            added_at: None,
            list_index: 0,
        }
    }

    pub fn duration_str(&self) -> String {
        ms_to_hms(self.duration)
    }
}

impl fmt::Display for Episode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl ListItem for Episode {
    fn is_playing(&self, queue: &Queue) -> bool {
        let current = queue.get_current();
        current
            .map(|t| t.id().as_deref() == Some(&self.id))
            .unwrap_or(false)
    }

    fn display_left(&self, _library: &Library) -> String {
        format!("{self}")
    }

    fn display_right(&self, _library: &Library) -> String {
        format!("{} {}", self.release_date, self.duration_str())
    }

    fn play(&mut self, queue: &Queue) {
        let index = queue.append_next(&vec![Playable::Episode(self.clone())]);
        queue.play(index, true, false);
    }

    fn play_next(&mut self, queue: &Queue) {
        queue.insert_after_current(Playable::Episode(self.clone()));
    }

    fn queue(&mut self, queue: &Queue) {
        queue.append(Playable::Episode(self.clone()));
    }

    fn toggle_saved(&mut self, _library: &Library) {
        // Episodes don't have a saved state in YouTube Music
    }

    fn save(&mut self, _library: &Library) {}
    fn unsave(&mut self, _library: &Library) {}

    fn open(&self, _queue: Arc<Queue>, _library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        None
    }

    fn share_url(&self) -> Option<String> {
        Some(format!("https://music.youtube.com/watch?v={}", self.id))
    }

    #[inline]
    fn is_playable(&self) -> bool {
        true
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
