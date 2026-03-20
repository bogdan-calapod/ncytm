use std::sync::Arc;

use crate::{
    library::Library,
    queue::Queue,
    traits::{IntoBoxedViewExt, ListItem},
    ui::listview::ListView,
};

#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct Category {
    pub id: String,
    pub name: String,
}

impl Category {
    pub fn new(id: String, name: String) -> Self {
        Self { id, name }
    }
}

impl ListItem for Category {
    fn is_playing(&self, _queue: &Queue) -> bool {
        false
    }

    fn display_left(&self, _library: &Library) -> String {
        self.name.clone()
    }

    fn display_right(&self, _library: &Library) -> String {
        String::new()
    }

    fn play(&mut self, _queue: &Queue) {}
    fn play_next(&mut self, _queue: &Queue) {}
    fn queue(&mut self, _queue: &Queue) {}
    fn toggle_saved(&mut self, _library: &Library) {}
    fn save(&mut self, _library: &Library) {}
    fn unsave(&mut self, _library: &Library) {}

    fn open(
        &self,
        queue: Arc<Queue>,
        library: Arc<Library>,
    ) -> Option<Box<dyn crate::traits::ViewExt>> {
        let playlists = queue.get_spotify().api.category_playlists(&self.id, 0);
        let view = ListView::new(
            Arc::new(std::sync::RwLock::new(playlists.items)),
            queue,
            library,
        )
        .with_title(&self.name);
        Some(view.into_boxed_view_ext())
    }

    fn share_url(&self) -> Option<String> {
        Some(format!("https://music.youtube.com/browse/{}", self.id))
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
