use std::sync::{Arc, RwLock};

use cursive::Cursive;
use cursive::view::ViewWrapper;

use crate::application::ASYNC_RUNTIME;
use crate::command::Command;
use crate::commands::CommandResult;
use crate::events::EventManager;
use crate::library::Library;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::playlist::Playlist;
use crate::model::track::Track;
use crate::queue::Queue;
use crate::spotify::{Spotify, UriType};
use crate::traits::{ListItem, ViewExt};
use crate::ui::listview::ListView;
use crate::ui::pagination::Pagination;
use crate::ui::tabbedview::TabbedView;
use crate::youtube_url::YouTubeUrl;

pub struct SearchResultsView {
    search_term: String,
    results_tracks: Arc<RwLock<Vec<Track>>>,
    results_albums: Arc<RwLock<Vec<Album>>>,
    results_artists: Arc<RwLock<Vec<Artist>>>,
    results_playlists: Arc<RwLock<Vec<Playlist>>>,
    tabs: TabbedView,
    spotify: Spotify,
    library: Arc<Library>,
    events: EventManager,
}

type SearchHandler<I> =
    Box<dyn Fn(&Spotify, &Arc<RwLock<Vec<I>>>, &str, usize, bool) -> u32 + Send + Sync>;

impl SearchResultsView {
    pub fn new(
        search_term: String,
        events: EventManager,
        queue: Arc<Queue>,
        library: Arc<Library>,
    ) -> Self {
        let results_tracks = Arc::new(RwLock::new(Vec::new()));
        let results_albums = Arc::new(RwLock::new(Vec::new()));
        let results_artists = Arc::new(RwLock::new(Vec::new()));
        let results_playlists = Arc::new(RwLock::new(Vec::new()));

        let list_tracks = ListView::new(results_tracks.clone(), queue.clone(), library.clone());
        let list_albums = ListView::new(results_albums.clone(), queue.clone(), library.clone());
        let list_artists = ListView::new(results_artists.clone(), queue.clone(), library.clone());
        let list_playlists =
            ListView::new(results_playlists.clone(), queue.clone(), library.clone());

        let mut tabs = TabbedView::new();
        tabs.add_tab("Tracks", list_tracks);
        tabs.add_tab("Albums", list_albums);
        tabs.add_tab("Artists", list_artists);
        tabs.add_tab("Playlists", list_playlists);

        let mut view = Self {
            search_term,
            results_tracks,
            results_albums,
            results_artists,
            results_playlists,
            tabs,
            spotify: queue.get_spotify(),
            library,
            events,
        };

        view.run_search();
        view
    }

    fn get_track(
        spotify: &Spotify,
        tracks: &Arc<RwLock<Vec<Track>>>,
        query: &str,
        _offset: usize,
        _append: bool,
    ) -> u32 {
        if let Some(result) = spotify.api.track(query) {
            let mut r = tracks.write().unwrap();
            *r = vec![result];
            return 1;
        }
        0
    }

    fn get_album(
        spotify: &Spotify,
        albums: &Arc<RwLock<Vec<Album>>>,
        query: &str,
        _offset: usize,
        _append: bool,
    ) -> u32 {
        if let Ok(result) = spotify.api.album(query) {
            let mut r = albums.write().unwrap();
            *r = vec![result];
            return 1;
        }
        0
    }

    fn get_artist(
        spotify: &Spotify,
        artists: &Arc<RwLock<Vec<Artist>>>,
        query: &str,
        _offset: usize,
        _append: bool,
    ) -> u32 {
        if let Some(result) = spotify.api.artist(query) {
            let mut r = artists.write().unwrap();
            *r = vec![result];
            return 1;
        }
        0
    }

    fn get_playlist(
        spotify: &Spotify,
        playlists: &Arc<RwLock<Vec<Playlist>>>,
        query: &str,
        _offset: usize,
        _append: bool,
    ) -> u32 {
        if let Some(result) = spotify.api.playlist(query) {
            let mut r = playlists.write().unwrap();
            *r = vec![result];
            return 1;
        }
        0
    }

    fn perform_search<I: ListItem + Clone>(
        &self,
        handler: SearchHandler<I>,
        results: &Arc<RwLock<Vec<I>>>,
        query: &str,
        paginator: Option<&Pagination<I>>,
    ) {
        let spotify = self.spotify.clone();
        let query = query.to_owned();
        let results = results.clone();
        let ev = self.events.clone();
        let paginator = paginator.cloned();

        std::thread::spawn(move || {
            let total_items = handler(&spotify, &results, &query, 0, false) as usize;

            // register paginator if the API has more than one page of results
            if let Some(mut paginator) = paginator {
                let loaded_items = results.read().unwrap().len();
                if total_items > loaded_items {
                    let ev = ev.clone();

                    // paginator callback
                    let cb = move |items: Arc<RwLock<Vec<I>>>| {
                        let offset = items.read().unwrap().len();
                        handler(&spotify, &results, &query, offset, true);
                        ev.trigger();
                    };
                    paginator.set(loaded_items, total_items, Box::new(cb));
                } else {
                    paginator.clear()
                }
            }
            ev.trigger();
        });
    }

    pub fn run_search(&mut self) {
        let query = self.search_term.clone();

        // check if API token refresh is necessary before commencing multiple
        // requests to avoid deadlock, as the parallel requests might
        // simultaneously try to refresh the token
        self.spotify
            .api
            .update_token()
            .map(move |h| ASYNC_RUNTIME.get().unwrap().block_on(h).ok());

        // is the query a YouTube Music URI?
        if let Ok(uritype) = query.parse() {
            match uritype {
                UriType::Track => {
                    self.perform_search(
                        Box::new(Self::get_track),
                        &self.results_tracks,
                        &query,
                        None,
                    );
                    self.tabs.set_selected(0);
                }
                UriType::Album => {
                    self.perform_search(
                        Box::new(Self::get_album),
                        &self.results_albums,
                        &query,
                        None,
                    );
                    self.tabs.set_selected(1);
                }
                UriType::Artist => {
                    self.perform_search(
                        Box::new(Self::get_artist),
                        &self.results_artists,
                        &query,
                        None,
                    );
                    self.tabs.set_selected(2);
                }
                UriType::Playlist => {
                    self.perform_search(
                        Box::new(Self::get_playlist),
                        &self.results_playlists,
                        &query,
                        None,
                    );
                    self.tabs.set_selected(3);
                }
                // Shows and Episodes not supported in YouTube Music
                UriType::Show | UriType::Episode => {}
            }
        // Is the query a YouTube Music URL?
        // https://music.youtube.com/watch?v=dQw4w9WgXcQ
        } else if let Some(url) = YouTubeUrl::from_url(&query) {
            match url.uri_type {
                UriType::Track => {
                    self.perform_search(
                        Box::new(Self::get_track),
                        &self.results_tracks,
                        &url.id,
                        None,
                    );
                    self.tabs.set_selected(0);
                }
                UriType::Album => {
                    self.perform_search(
                        Box::new(Self::get_album),
                        &self.results_albums,
                        &url.id,
                        None,
                    );
                    self.tabs.set_selected(1);
                }
                UriType::Artist => {
                    self.perform_search(
                        Box::new(Self::get_artist),
                        &self.results_artists,
                        &url.id,
                        None,
                    );
                    self.tabs.set_selected(2);
                }
                UriType::Playlist => {
                    self.perform_search(
                        Box::new(Self::get_playlist),
                        &self.results_playlists,
                        &url.id,
                        None,
                    );
                    self.tabs.set_selected(3);
                }
                // Shows and Episodes not supported in YouTube Music
                UriType::Show | UriType::Episode => {}
            }
        } else {
            // Use YouTube Music search via Library
            self.perform_yt_search(&query);
        }
    }

    /// Perform YouTube Music search using the Library's search method.
    fn perform_yt_search(&self, query: &str) {
        let library = self.library.clone();
        let query = query.to_owned();
        let results_tracks = self.results_tracks.clone();
        let results_albums = self.results_albums.clone();
        let results_artists = self.results_artists.clone();
        let results_playlists = self.results_playlists.clone();
        let ev = self.events.clone();

        std::thread::spawn(move || {
            let search_results = library.search(&query);

            // Convert and store tracks
            {
                let tracks: Vec<Track> = search_results
                    .tracks
                    .iter()
                    .enumerate()
                    .map(|(i, t)| Library::search_track_to_track(t, i))
                    .collect();
                *results_tracks.write().unwrap() = tracks;
            }

            // Convert and store albums
            {
                let albums: Vec<Album> = search_results
                    .albums
                    .iter()
                    .map(Library::search_album_to_album)
                    .collect();
                *results_albums.write().unwrap() = albums;
            }

            // Convert and store artists
            {
                let artists: Vec<Artist> = search_results
                    .artists
                    .iter()
                    .map(Library::search_artist_to_artist)
                    .collect();
                *results_artists.write().unwrap() = artists;
            }

            // Convert and store playlists
            {
                let playlists: Vec<Playlist> = search_results
                    .playlists
                    .iter()
                    .map(Library::search_playlist_to_playlist)
                    .collect();
                *results_playlists.write().unwrap() = playlists;
            }

            ev.trigger();
        });
    }
}

impl ViewWrapper for SearchResultsView {
    wrap_impl!(self.tabs: TabbedView);
}

impl ViewExt for SearchResultsView {
    fn title(&self) -> String {
        format!("Search: {}", self.search_term)
    }
    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        self.tabs.on_command(s, cmd)
    }
}
