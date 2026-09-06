use std::error::Error;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use cursive::traits::Nameable;
use cursive::{Cursive, CursiveRunner};
use log::{error, info};

#[cfg(unix)]
use signal_hook::{consts::SIGHUP, consts::SIGTERM, iterator::Signals};

use crate::command::Command;
use crate::commands::CommandManager;
use crate::config::{Config, PlaybackState};
use crate::events::{Event, EventManager};
use crate::library::Library;
use crate::queue::Queue;
use crate::spotify::Spotify;
use crate::ui::create_cursive;
use crate::youtube_music::YouTubeMusicClient;
use crate::{authentication, ui, utils};
use crate::{command, queue, spotify};

#[cfg(feature = "mpris")]
use crate::mpris::MprisManager;

#[cfg(unix)]
use crate::ipc::{self, IpcSocket};

#[cfg(all(target_os = "macos", feature = "media_control"))]
use crate::macos_event_loop::{MediaControlEvent, MediaControlHandle};

/// Set up the global logger to log to `filename`.
pub fn setup_logging(filename: &Path) -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        // Perform allocation-free log formatting
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} [{}] [{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        // Add blanket level filter -
        .level(log::LevelFilter::Debug)
        // Set runtime log level for modules
        .level_for("ncytm", log::LevelFilter::Trace)
        // Output to stdout, files, and other Dispatch configurations
        .chain(fern::log_file(filename)?)
        // Apply globally
        .apply()?;
    Ok(())
}

pub type UserData = Rc<UserDataInner>;
pub struct UserDataInner {
    pub cmd: CommandManager,
}

/// The global Tokio runtime for running asynchronous tasks.
pub static ASYNC_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// The representation of an ncytm application.
pub struct Application {
    /// The music queue which controls playback order.
    queue: Arc<Queue>,
    /// Internally shared
    spotify: Spotify,
    /// Internally shared
    event_manager: EventManager,
    /// An IPC implementation using Unix domain sockets, used to control ncytm.
    /// The field is kept alive for RAII cleanup of the socket file.
    #[cfg(unix)]
    _ipc: Option<IpcSocket>,
    /// The object to render to the terminal.
    cursive: CursiveRunner<Cursive>,
    /// macOS media control handle for sending metadata/playback updates
    #[cfg(all(target_os = "macos", feature = "media_control"))]
    media_handle: Option<MediaControlHandle>,
    /// macOS media control events receiver
    #[cfg(all(target_os = "macos", feature = "media_control"))]
    media_events: Option<std::sync::mpsc::Receiver<MediaControlEvent>>,
    /// Last known track ID for detecting track changes (used to update media metadata)
    #[cfg(all(target_os = "macos", feature = "media_control"))]
    last_track_id: Option<String>,
    /// Whether playback was in the `Playing` state on the previous loop
    /// iteration, used to detect resume-from-pause for media focus reclaim.
    #[cfg(all(target_os = "macos", feature = "media_control"))]
    was_playing: bool,
    /// The track ID for which media focus was last reclaimed, so we reclaim
    /// once per track once it starts playing rather than on every loop tick.
    #[cfg(all(target_os = "macos", feature = "media_control"))]
    reclaimed_track_id: Option<String>,
    /// Slack now-playing status integration.
    #[cfg(feature = "slack_status")]
    slack: Option<crate::slack::SlackStatus>,
    /// Last track ID pushed to Slack, for detecting track changes.
    #[cfg(feature = "slack_status")]
    slack_last_track_id: Option<String>,
    /// Whether Slack currently shows a track (so we only clear once).
    #[cfg(feature = "slack_status")]
    slack_showing: bool,
    /// Optional floating album-art thumbnail rendered near the statusbar.
    #[cfg(feature = "cover")]
    status_cover: Option<ui::cover::StatusCover>,
    /// Throttle + cache for the (relatively expensive) tmux pane-visibility
    /// check, so we don't spawn a `tmux` subprocess on every loop iteration.
    #[cfg(feature = "cover")]
    pane_visible_cache: std::cell::Cell<(std::time::Instant, bool)>,
}

impl Application {
    /// Create a new ncspot application.
    ///
    /// # Arguments
    ///
    /// * `configuration_file_path` - Relative path to the configuration file inside the base path
    /// * `media_handle` - (macOS only) Handle to send metadata/playback updates to media controls
    /// * `media_events` - (macOS only) Receiver for media control events (play/pause/next/prev)
    #[cfg(all(target_os = "macos", feature = "media_control"))]
    pub fn new(
        configuration_file_path: Option<String>,
        media_handle: Option<MediaControlHandle>,
        media_events: Option<std::sync::mpsc::Receiver<MediaControlEvent>>,
    ) -> Result<Self, Box<dyn Error>> {
        Self::new_inner(configuration_file_path, media_handle, media_events)
    }

    /// Create a new ncspot application.
    ///
    /// # Arguments
    ///
    /// * `configuration_file_path` - Relative path to the configuration file inside the base path
    #[cfg(not(all(target_os = "macos", feature = "media_control")))]
    pub fn new(configuration_file_path: Option<String>) -> Result<Self, Box<dyn Error>> {
        Self::new_inner(configuration_file_path)
    }

    #[cfg(all(target_os = "macos", feature = "media_control"))]
    fn new_inner(
        configuration_file_path: Option<String>,
        media_handle: Option<MediaControlHandle>,
        media_events: Option<std::sync::mpsc::Receiver<MediaControlEvent>>,
    ) -> Result<Self, Box<dyn Error>> {
        // Things here may cause the process to abort; we must do them before creating curses
        // windows otherwise the error message will not be seen by a user

        ASYNC_RUNTIME
            .set(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .unwrap(),
            )
            .unwrap();

        let configuration = Arc::new(Config::new(configuration_file_path));
        let theme = configuration.build_theme();

        // Authenticate with YouTube Music using cookies
        let auth_result = match authentication::authenticate(&configuration) {
            Ok(result) => {
                info!(
                    "Authenticated as: {}",
                    result.account_name.as_deref().unwrap_or("Unknown")
                );
                result
            }
            Err(e) => {
                eprintln!("Authentication failed: {}", e);
                eprintln!("{}", authentication::get_cookie_instructions());
                return Err(e.to_string().into());
            }
        };

        // Create credentials for backward compatibility with Spotify stub
        let credentials = spotify::Credentials {};

        println!("Connecting to YouTube Music..");

        // DON'T USE STDOUT AFTER THIS CALL!
        let mut cursive = create_cursive().map_err(|error| error.to_string())?;

        cursive.set_theme(theme.clone());

        #[cfg(all(unix, feature = "pancurses_backend"))]
        cursive.add_global_callback(cursive::event::Event::CtrlChar('z'), |_s| unsafe {
            libc::raise(libc::SIGTSTP);
        });

        let event_manager = EventManager::new(cursive.cb_sink().clone());

        let mut spotify =
            spotify::Spotify::new(event_manager.clone(), credentials, configuration.clone())?;

        // Set cookies for playback and start the player worker
        spotify.set_cookies(auth_result.cookies.clone());
        spotify.start_worker(None)?;

        // Create YouTube Music client for library operations
        let yt_client = YouTubeMusicClient::new(auth_result.cookies)
            .map_err(|e| format!("Failed to create YouTube Music client: {}", e))?;

        let library = Arc::new(Library::new_with_client(
            event_manager.clone(),
            spotify.clone(),
            yt_client,
            configuration.clone(),
        ));

        let queue = Arc::new(queue::Queue::new(
            spotify.clone(),
            configuration.clone(),
            library.clone(),
        ));

        #[cfg(feature = "mpris")]
        let mpris_manager = MprisManager::new(
            event_manager.clone(),
            queue.clone(),
            library.clone(),
            spotify.clone(),
        );

        #[cfg(feature = "mpris")]
        spotify.set_mpris(mpris_manager.clone());

        // Load the last played track into the player
        let playback_state = configuration.state().playback_state.clone();
        let queue_state = configuration.state().queuestate.clone();

        if let Some(playable) = queue.get_current() {
            spotify.load(
                &playable,
                playback_state == PlaybackState::Playing,
                queue_state.track_progress.as_millis() as u32,
            );
            spotify.update_track();
            match playback_state {
                PlaybackState::Stopped => {
                    spotify.stop();
                }
                PlaybackState::Paused | PlaybackState::Playing | PlaybackState::Default => {
                    spotify.pause();
                }
            }
        }

        #[cfg(unix)]
        let ipc = if let Ok(runtime_directory) = utils::create_runtime_directory() {
            Some(
                ipc::IpcSocket::new(
                    ASYNC_RUNTIME.get().unwrap().handle(),
                    runtime_directory.join("ncytm.sock"),
                    event_manager.clone(),
                )
                .map_err(|e| e.to_string())?,
            )
        } else {
            error!("failed to create IPC socket: no suitable user runtime directory found");
            None
        };

        let mut cmd_manager = CommandManager::new(
            spotify.clone(),
            queue.clone(),
            library.clone(),
            configuration.clone(),
            event_manager.clone(),
        );

        cmd_manager.register_all();
        cmd_manager.register_keybindings(&mut cursive);

        cursive.set_user_data(Rc::new(UserDataInner { cmd: cmd_manager }));

        let search =
            ui::search::SearchView::new(event_manager.clone(), queue.clone(), library.clone());

        let libraryview = ui::library::LibraryView::new(queue.clone(), library.clone());

        let queueview = ui::queue::QueueView::new(queue.clone(), library.clone());

        #[cfg(feature = "cover")]
        let coverview = ui::cover::CoverView::new(queue.clone(), library.clone(), &configuration);

        let status = ui::statusbar::StatusBar::new(queue.clone(), Arc::clone(&library));

        let mut layout =
            ui::layout::Layout::new(status, &event_manager, theme, Arc::clone(&configuration))
                .screen("search", search.with_name("search"))
                .screen("library", libraryview.with_name("library"))
                .screen("queue", queueview);

        #[cfg(feature = "cover")]
        layout.add_screen("cover", coverview.with_name("cover"));

        // Optional floating thumbnail near the statusbar, only when enabled and
        // the terminal supports an overlay image protocol.
        #[cfg(feature = "cover")]
        let status_cover = if configuration.values().status_cover.unwrap_or(false)
            && ui::cover::status_thumbnail_supported()
        {
            Some(ui::cover::StatusCover::new(queue.clone(), &configuration))
        } else {
            None
        };

        // initial screen is library
        let initial_screen = configuration
            .values()
            .initial_screen
            .clone()
            .unwrap_or_else(|| "library".to_string());
        if layout.has_screen(&initial_screen) {
            layout.set_screen(initial_screen);
        } else {
            error!("Invalid screen name: {initial_screen}");
            layout.set_screen("library");
        }

        cursive.add_fullscreen_layer(layout.with_name("main"));

        // Send initial metadata if we have a current track and media handle
        #[cfg(all(target_os = "macos", feature = "media_control"))]
        if let (Some(handle), Some(playable)) = (&media_handle, queue.get_current()) {
            use crate::model::playable::Playable;
            match playable {
                Playable::Track(track) => {
                    handle.set_metadata(
                        Some(&track.title),
                        track.artists.first().map(|s: &String| s.as_str()),
                        track.album.as_deref(),
                        Some(track.duration as u64),
                        track.cover_url.as_deref(),
                    );
                }
            }
        }

        Ok(Self {
            queue,
            spotify,
            event_manager,
            #[cfg(unix)]
            _ipc: ipc,
            cursive,
            #[cfg(all(target_os = "macos", feature = "media_control"))]
            media_handle,
            #[cfg(all(target_os = "macos", feature = "media_control"))]
            media_events,
            #[cfg(all(target_os = "macos", feature = "media_control"))]
            last_track_id: None,
            #[cfg(all(target_os = "macos", feature = "media_control"))]
            was_playing: false,
            #[cfg(all(target_os = "macos", feature = "media_control"))]
            reclaimed_track_id: None,
            #[cfg(feature = "slack_status")]
            slack: crate::slack::SlackStatus::new(configuration.values().slack_status.as_ref()),
            #[cfg(feature = "slack_status")]
            slack_last_track_id: None,
            #[cfg(feature = "slack_status")]
            slack_showing: false,
            #[cfg(feature = "cover")]
            status_cover,
            #[cfg(feature = "cover")]
            pane_visible_cache: std::cell::Cell::new((std::time::Instant::now(), true)),
        })
    }

    #[cfg(not(all(target_os = "macos", feature = "media_control")))]
    fn new_inner(configuration_file_path: Option<String>) -> Result<Self, Box<dyn Error>> {
        // Things here may cause the process to abort; we must do them before creating curses
        // windows otherwise the error message will not be seen by a user

        ASYNC_RUNTIME
            .set(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .unwrap(),
            )
            .unwrap();

        let configuration = Arc::new(Config::new(configuration_file_path));
        let theme = configuration.build_theme();

        // Authenticate with YouTube Music using cookies
        let auth_result = match authentication::authenticate(&configuration) {
            Ok(result) => {
                info!(
                    "Authenticated as: {}",
                    result.account_name.as_deref().unwrap_or("Unknown")
                );
                result
            }
            Err(e) => {
                eprintln!("Authentication failed: {}", e);
                eprintln!("{}", authentication::get_cookie_instructions());
                return Err(e.to_string().into());
            }
        };

        // Create credentials for backward compatibility with Spotify stub
        let credentials = spotify::Credentials {};

        println!("Connecting to YouTube Music..");

        // DON'T USE STDOUT AFTER THIS CALL!
        let mut cursive = create_cursive().map_err(|error| error.to_string())?;

        cursive.set_theme(theme.clone());

        #[cfg(all(unix, feature = "pancurses_backend"))]
        cursive.add_global_callback(cursive::event::Event::CtrlChar('z'), |_s| unsafe {
            libc::raise(libc::SIGTSTP);
        });

        let event_manager = EventManager::new(cursive.cb_sink().clone());

        let mut spotify =
            spotify::Spotify::new(event_manager.clone(), credentials, configuration.clone())?;

        // Set cookies for playback and start the player worker
        spotify.set_cookies(auth_result.cookies.clone());
        spotify.start_worker(None)?;

        // Create YouTube Music client for library operations
        let yt_client = YouTubeMusicClient::new(auth_result.cookies)
            .map_err(|e| format!("Failed to create YouTube Music client: {}", e))?;

        let library = Arc::new(Library::new_with_client(
            event_manager.clone(),
            spotify.clone(),
            yt_client,
            configuration.clone(),
        ));

        let queue = Arc::new(queue::Queue::new(
            spotify.clone(),
            configuration.clone(),
            library.clone(),
        ));

        #[cfg(feature = "mpris")]
        let mpris_manager = MprisManager::new(
            event_manager.clone(),
            queue.clone(),
            library.clone(),
            spotify.clone(),
        );

        #[cfg(feature = "mpris")]
        spotify.set_mpris(mpris_manager.clone());

        // Load the last played track into the player
        let playback_state = configuration.state().playback_state.clone();
        let queue_state = configuration.state().queuestate.clone();

        if let Some(playable) = queue.get_current() {
            spotify.load(
                &playable,
                playback_state == PlaybackState::Playing,
                queue_state.track_progress.as_millis() as u32,
            );
            spotify.update_track();
            match playback_state {
                PlaybackState::Stopped => {
                    spotify.stop();
                }
                PlaybackState::Paused | PlaybackState::Playing | PlaybackState::Default => {
                    spotify.pause();
                }
            }
        }

        #[cfg(unix)]
        let ipc = if let Ok(runtime_directory) = utils::create_runtime_directory() {
            Some(
                ipc::IpcSocket::new(
                    ASYNC_RUNTIME.get().unwrap().handle(),
                    runtime_directory.join("ncytm.sock"),
                    event_manager.clone(),
                )
                .map_err(|e| e.to_string())?,
            )
        } else {
            error!("failed to create IPC socket: no suitable user runtime directory found");
            None
        };

        let mut cmd_manager = CommandManager::new(
            spotify.clone(),
            queue.clone(),
            library.clone(),
            configuration.clone(),
            event_manager.clone(),
        );

        cmd_manager.register_all();
        cmd_manager.register_keybindings(&mut cursive);

        cursive.set_user_data(Rc::new(UserDataInner { cmd: cmd_manager }));

        let search =
            ui::search::SearchView::new(event_manager.clone(), queue.clone(), library.clone());

        let libraryview = ui::library::LibraryView::new(queue.clone(), library.clone());

        let queueview = ui::queue::QueueView::new(queue.clone(), library.clone());

        #[cfg(feature = "cover")]
        let coverview = ui::cover::CoverView::new(queue.clone(), library.clone(), &configuration);

        let status = ui::statusbar::StatusBar::new(queue.clone(), Arc::clone(&library));

        let mut layout =
            ui::layout::Layout::new(status, &event_manager, theme, Arc::clone(&configuration))
                .screen("search", search.with_name("search"))
                .screen("library", libraryview.with_name("library"))
                .screen("queue", queueview);

        #[cfg(feature = "cover")]
        layout.add_screen("cover", coverview.with_name("cover"));

        // Optional floating thumbnail near the statusbar, only when enabled and
        // the terminal supports an overlay image protocol.
        #[cfg(feature = "cover")]
        let status_cover = if configuration.values().status_cover.unwrap_or(false)
            && ui::cover::status_thumbnail_supported()
        {
            Some(ui::cover::StatusCover::new(queue.clone(), &configuration))
        } else {
            None
        };

        // initial screen is library
        let initial_screen = configuration
            .values()
            .initial_screen
            .clone()
            .unwrap_or_else(|| "library".to_string());
        if layout.has_screen(&initial_screen) {
            layout.set_screen(initial_screen);
        } else {
            error!("Invalid screen name: {initial_screen}");
            layout.set_screen("library");
        }

        cursive.add_fullscreen_layer(layout.with_name("main"));

        Ok(Self {
            queue,
            spotify,
            event_manager,
            #[cfg(unix)]
            _ipc: ipc,
            cursive,
            #[cfg(feature = "slack_status")]
            slack: crate::slack::SlackStatus::new(configuration.values().slack_status.as_ref()),
            #[cfg(feature = "slack_status")]
            slack_last_track_id: None,
            #[cfg(feature = "slack_status")]
            slack_showing: false,
            #[cfg(feature = "cover")]
            status_cover,
            #[cfg(feature = "cover")]
            pane_visible_cache: std::cell::Cell::new((std::time::Instant::now(), true)),
        })
    }

    /// Update macOS media control metadata for the current track
    #[cfg(all(target_os = "macos", feature = "media_control"))]
    fn update_media_metadata(&self) {
        use crate::macos_event_loop::PlaybackState as MediaPlaybackState;
        use crate::model::playable::Playable;

        if let Some(ref handle) = self.media_handle
            && let Some(playable) = self.queue.get_current()
        {
            match playable {
                Playable::Track(track) => {
                    handle.set_metadata(
                        Some(&track.title),
                        track.artists.first().map(|s: &String| s.as_str()),
                        track.album.as_deref(),
                        Some(track.duration as u64),
                        track.cover_url.as_deref(),
                    );
                }
            }
            // Also update playback state to playing
            let progress_secs = self.spotify.get_current_progress().as_secs_f64();
            handle.set_playback(MediaPlaybackState::Playing {
                progress_secs: Some(progress_secs),
            });
        }
    }

    /// Reclaim the macOS "Now Playing" slot for ncytm.
    ///
    /// When another app plays media after ncytm, macOS hands the Control Center
    /// "Now Playing" widget to that app. Re-registering our remote command
    /// handlers and re-asserting the current track prompts macOS to re-elect
    /// ncytm — the same thing that happens on a fresh launch, without requiring
    /// a restart. Only meaningful while a track is playing.
    #[cfg(all(target_os = "macos", feature = "media_control"))]
    fn reclaim_media_focus(&self) {
        use crate::model::playable::Playable;

        if let Some(ref handle) = self.media_handle
            && let Some(Playable::Track(track)) = self.queue.get_current()
        {
            // Re-attach the remote command handlers and force a real
            // Paused -> Playing transition with fresh metadata, which prompts
            // macOS to re-elect ncytm as the current Now Playing app.
            info!("Reclaiming macOS Now Playing focus for: {}", track.title);
            let progress_secs = self.spotify.get_current_progress().as_secs_f64();
            handle.reclaim(
                Some(&track.title),
                track.artists.first().map(|s| s.as_str()),
                track.album.as_deref(),
                Some(track.duration as u64),
                track.cover_url.as_deref(),
                Some(progress_secs),
            );
        }
    }

    /// Reconcile the Slack status with the current playback state.
    ///
    /// Only acts on track changes (while playing) and on pause/stop, matching
    /// the desired semantics: we never touch Slack on a timer.
    #[cfg(feature = "slack_status")]
    fn update_slack_status(&mut self) {
        use crate::model::playable::Playable;
        use crate::spotify::PlayerEvent;

        let Some(slack) = self.slack.as_ref() else {
            return;
        };

        let status = self.spotify.get_current_status();
        let current = self.queue.get_current();
        let current_id = current.as_ref().and_then(|p| p.id());

        match status {
            PlayerEvent::Playing(_) => {
                // Update on track change (or when we transition into playing a
                // track we haven't shown yet).
                if (current_id != self.slack_last_track_id || !self.slack_showing)
                    && let Some(Playable::Track(track)) = current
                {
                    slack.update(&track);
                    self.slack_last_track_id = current_id;
                    self.slack_showing = true;
                }
            }
            PlayerEvent::Paused(_) | PlayerEvent::Stopped | PlayerEvent::FailedToPlay(_) => {
                // Strip our addition on pause/stop, but only once.
                if self.slack_showing {
                    slack.clear();
                    self.slack_showing = false;
                    self.slack_last_track_id = None;
                }
            }
            PlayerEvent::Loading => {}
        }
    }

    /// Clear ncytm's addition from the Slack status, if shown. Used on shutdown.
    #[cfg(feature = "slack_status")]
    fn clear_slack_status(&mut self) {
        if let Some(slack) = self.slack.as_ref()
            && self.slack_showing
        {
            slack.clear();
            self.slack_showing = false;
            self.slack_last_track_id = None;
        }
    }

    /// Return whether ncytm's tmux pane is currently visible, caching the
    /// result briefly to avoid spawning a `tmux` process on every loop tick.
    #[cfg(feature = "cover")]
    fn tmux_pane_visible_throttled(&self) -> bool {
        const TTL: std::time::Duration = std::time::Duration::from_millis(250);
        let (checked_at, cached) = self.pane_visible_cache.get();
        if checked_at.elapsed() < TTL {
            return cached;
        }
        let visible = ui::cover::tmux_pane_visible();
        self.pane_visible_cache
            .set((std::time::Instant::now(), visible));
        visible
    }

    /// Start the application and run the event loop.
    pub fn run(&mut self) -> Result<(), String> {
        #[cfg(unix)]
        let mut signals = {
            let sigs = [SIGTERM, SIGHUP];
            Signals::new(sigs).expect("could not register signal handler")
        };

        // cursive event loop
        while self.cursive.is_running() {
            self.cursive.step();

            // Terminal graphics are drawn to the physical terminal and aren't
            // tracked by tmux, so hide them whenever our tmux pane isn't the
            // one on screen. The check spawns a `tmux` process, so throttle it.
            #[cfg(feature = "cover")]
            let pane_visible = self.tmux_pane_visible_throttled();

            // Render cover art after the Cursive frame has flushed so the UI
            // does not overwrite the image.
            #[cfg(feature = "cover")]
            self.cursive
                .call_on_name("cover", |view: &mut ui::cover::CoverView| {
                    view.set_visible(pane_visible);
                    view.render_to_terminal();
                });

            // Render the floating status thumbnail (if enabled). It is
            // suppressed while the full cover screen is focused so the two
            // images don't fight over the terminal.
            #[cfg(feature = "cover")]
            if let Some(status_cover) = self.status_cover.as_ref() {
                let cover_focused = self
                    .cursive
                    .call_on_name("main", |layout: &mut ui::layout::Layout| {
                        layout.is_screen_focused("cover")
                    })
                    .unwrap_or(false);
                status_cover.update(cover_focused || !pane_visible);
                status_cover.render_to_terminal();
            }

            // Process macOS media control events
            #[cfg(all(target_os = "macos", feature = "media_control"))]
            if let Some(ref media_events) = self.media_events {
                while let Ok(event) = media_events.try_recv() {
                    log::trace!("media control event: {:?}", event);
                    match event {
                        MediaControlEvent::Play => {
                            self.spotify.play();
                        }
                        MediaControlEvent::Pause => {
                            self.spotify.pause();
                        }
                        MediaControlEvent::Toggle => {
                            self.spotify.toggleplayback();
                        }
                        MediaControlEvent::Next => {
                            self.queue.next(false);
                            self.update_media_metadata();
                        }
                        MediaControlEvent::Previous => {
                            // Check if we should go to previous track or restart current track
                            // If less than 15% played, go to previous track, otherwise restart
                            let should_go_previous = if let Some(current) = self.queue.get_current()
                            {
                                let duration_secs = current.duration(); // duration is in seconds
                                let progress_secs = self.spotify.get_current_progress().as_secs();
                                let threshold_secs = (duration_secs as f32 * 0.15) as u64;
                                progress_secs < threshold_secs
                            } else {
                                true // If no current track, default to previous behavior
                            };

                            if should_go_previous {
                                self.queue.previous();
                            } else {
                                self.spotify.seek(0);
                            }
                            self.update_media_metadata();
                        }
                        MediaControlEvent::Stop => {
                            self.spotify.stop();
                        }
                        MediaControlEvent::SeekForward => {
                            self.spotify.seek_relative(10000); // 10 seconds
                        }
                        MediaControlEvent::SeekBackward => {
                            self.spotify.seek_relative(-10000); // -10 seconds
                        }
                        MediaControlEvent::SetPosition(secs) => {
                            self.spotify.seek((secs * 1000.0) as u32);
                        }
                    }
                }
            }

            // Check if track finished and advance to next
            if self.spotify.take_track_finished() {
                log::debug!("Track finished, advancing to next");
                self.queue.next(false);
            }

            // Reconcile the Slack now-playing status with playback state.
            #[cfg(feature = "slack_status")]
            self.update_slack_status();

            // Check if current track changed and update media metadata, and
            // reclaim the macOS "Now Playing" slot on track change / resume.
            #[cfg(all(target_os = "macos", feature = "media_control"))]
            {
                use crate::spotify::PlayerEvent;

                let is_playing =
                    matches!(self.spotify.get_current_status(), PlayerEvent::Playing(_));
                let current_track_id = self.queue.get_current().and_then(|p| p.id());
                let track_changed = current_track_id != self.last_track_id;

                if track_changed {
                    self.last_track_id = current_track_id.clone();
                    self.update_media_metadata();
                    // A newly loaded track hasn't been reclaimed yet; it will
                    // reclaim once it reaches the Playing state below.
                    self.reclaimed_track_id = None;
                }

                // Reclaim focus once a track reaches the Playing state. This
                // covers both a fresh track (which passes through Loading before
                // Playing) and resuming from pause. We reclaim once per
                // (track, play transition) to avoid spamming while playing.
                let resumed = is_playing && !self.was_playing;
                let not_yet_reclaimed = self.reclaimed_track_id != current_track_id;
                if is_playing && current_track_id.is_some() && (not_yet_reclaimed || resumed) {
                    self.reclaim_media_focus();
                    self.reclaimed_track_id = current_track_id;
                }
                self.was_playing = is_playing;
            }

            #[cfg(unix)]
            for signal in signals.pending() {
                if signal == SIGTERM || signal == SIGHUP {
                    info!("Caught {signal}, cleaning up and closing");
                    if let Some(data) = self.cursive.user_data::<UserData>().cloned() {
                        data.cmd.handle(&mut self.cursive, Command::Quit);
                    }
                }
            }
            for event in self.event_manager.msg_iter() {
                match event {
                    Event::IpcInput(input) => match command::parse(&input) {
                        Ok(commands) => {
                            if let Some(data) = self.cursive.user_data::<UserData>().cloned() {
                                for cmd in commands {
                                    info!("Executing command from IPC: {cmd}");
                                    data.cmd.handle(&mut self.cursive, cmd);
                                }
                            }
                        }
                        Err(e) => error!("Parsing error: {e}"),
                    },
                }
            }
        }

        // The event loop has exited (quit); restore the user's Slack status.
        #[cfg(feature = "slack_status")]
        self.clear_slack_status();

        // Remove any lingering floating thumbnail from the terminal.
        #[cfg(feature = "cover")]
        if let Some(status_cover) = self.status_cover.as_ref() {
            status_cover.clear();
        }

        Ok(())
    }
}
