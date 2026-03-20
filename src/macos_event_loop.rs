//! macOS main thread event loop for media control support.
//!
//! On macOS, media controls require an AppDelegate/winit event loop running on the main thread.
//! This module provides the infrastructure to run the app's TUI in a worker thread while
//! keeping the winit event loop on main.

use std::sync::{Mutex, OnceLock, mpsc};
use std::thread;

use log::{debug, warn};

/// Global media control handle for macOS
pub static MEDIA_HANDLE: OnceLock<MediaControlHandle> = OnceLock::new();

/// Global receiver for media control events
pub static MEDIA_EVENTS: OnceLock<Mutex<mpsc::Receiver<MediaControlEvent>>> = OnceLock::new();

/// Messages from the app to the media control event loop
pub enum MediaControlCommand {
    /// Update metadata (title, artist, album, duration, cover_url)
    SetMetadata {
        title: Option<String>,
        artist: Option<String>,
        album: Option<String>,
        duration_secs: Option<u64>,
        cover_url: Option<String>,
    },
    /// Update playback state
    SetPlayback(PlaybackState),
    /// Shutdown the event loop
    Shutdown,
}

/// Playback state for media controls
#[derive(Clone, Debug)]
pub enum PlaybackState {
    Playing { progress_secs: Option<f64> },
    Paused { progress_secs: Option<f64> },
    Stopped,
}

/// Events from media controls to the app
#[derive(Clone, Debug)]
pub enum MediaControlEvent {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    SeekForward,
    SeekBackward,
    SetPosition(f64),
}

/// Handle for sending commands to the media control event loop
#[derive(Clone)]
pub struct MediaControlHandle {
    tx: mpsc::Sender<MediaControlCommand>,
}

impl MediaControlHandle {
    pub fn set_metadata(
        &self,
        title: Option<&str>,
        artist: Option<&str>,
        album: Option<&str>,
        duration_secs: Option<u64>,
        cover_url: Option<&str>,
    ) {
        let _ = self.tx.send(MediaControlCommand::SetMetadata {
            title: title.map(String::from),
            artist: artist.map(String::from),
            album: album.map(String::from),
            duration_secs,
            cover_url: cover_url.map(String::from),
        });
    }

    pub fn set_playback(&self, state: PlaybackState) {
        let _ = self.tx.send(MediaControlCommand::SetPlayback(state));
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(MediaControlCommand::Shutdown);
    }
}

/// Run the application with the macOS event loop on main thread.
///
/// This function:
/// 1. Spawns the actual application in a worker thread
/// 2. Runs the winit event loop on the main thread (required for macOS media controls)
/// 3. Returns when either the app exits or the event loop is shut down
pub fn run_with_macos_event_loop<F>(app_fn: F) -> Result<(), String>
where
    F: FnOnce(MediaControlHandle, mpsc::Receiver<MediaControlEvent>) -> Result<(), String>
        + Send
        + 'static,
{
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::window::{Window, WindowId};

    use souvlaki::{MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig};
    use std::time::Duration;

    // Channel for app -> media controls
    let (cmd_tx, cmd_rx) = mpsc::channel::<MediaControlCommand>();
    // Channel for media controls -> app
    let (event_tx, event_rx) = mpsc::channel::<MediaControlEvent>();

    let handle = MediaControlHandle { tx: cmd_tx };

    // Spawn the app in a worker thread
    let app_handle = thread::spawn(move || app_fn(handle, event_rx));

    // Run winit on main thread
    struct App {
        window: Option<Window>,
        controls: Option<MediaControls>,
        cmd_rx: mpsc::Receiver<MediaControlCommand>,
        event_tx: mpsc::Sender<MediaControlEvent>,
        should_exit: bool,
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            debug!("winit: resumed event on main thread");

            // Create a hidden window (required for AppDelegate)
            let window_attrs = Window::default_attributes()
                .with_visible(false)
                .with_title("ncytm");

            match event_loop.create_window(window_attrs) {
                Ok(window) => {
                    debug!("winit: hidden window created");
                    self.window = Some(window);

                    // Now create media controls
                    let config = PlatformConfig {
                        dbus_name: "org.mpris.MediaPlayer2.ncytm",
                        display_name: "ncytm",
                        hwnd: None,
                    };

                    match MediaControls::new(config) {
                        Ok(mut controls) => {
                            debug!("winit: MediaControls created on main thread");

                            let tx = self.event_tx.clone();
                            if let Err(e) = controls.attach(move |e| {
                                let event = match e {
                                    souvlaki::MediaControlEvent::Play => MediaControlEvent::Play,
                                    souvlaki::MediaControlEvent::Pause => MediaControlEvent::Pause,
                                    souvlaki::MediaControlEvent::Toggle => {
                                        MediaControlEvent::Toggle
                                    }
                                    souvlaki::MediaControlEvent::Next => MediaControlEvent::Next,
                                    souvlaki::MediaControlEvent::Previous => {
                                        MediaControlEvent::Previous
                                    }
                                    souvlaki::MediaControlEvent::Stop => MediaControlEvent::Stop,
                                    souvlaki::MediaControlEvent::Seek(
                                        souvlaki::SeekDirection::Forward,
                                    ) => MediaControlEvent::SeekForward,
                                    souvlaki::MediaControlEvent::Seek(
                                        souvlaki::SeekDirection::Backward,
                                    ) => MediaControlEvent::SeekBackward,
                                    souvlaki::MediaControlEvent::SetPosition(
                                        souvlaki::MediaPosition(dur),
                                    ) => MediaControlEvent::SetPosition(dur.as_secs_f64()),
                                    _ => return,
                                };
                                let _ = tx.send(event);
                            }) {
                                warn!("winit: Failed to attach event handler: {:?}", e);
                            } else {
                                debug!("winit: Event handler attached");
                                // Initialize as playing
                                let _ = controls
                                    .set_playback(MediaPlayback::Playing { progress: None });
                                self.controls = Some(controls);
                            }
                        }
                        Err(e) => {
                            warn!("winit: Failed to create MediaControls: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("winit: Failed to create window: {}", e);
                }
            }
        }

        fn window_event(
            &mut self,
            _event_loop: &ActiveEventLoop,
            _id: WindowId,
            _event: WindowEvent,
        ) {
            // We don't care about window events
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            // Process pending commands from the app
            while let Ok(cmd) = self.cmd_rx.try_recv() {
                if let Some(ref mut controls) = self.controls {
                    match cmd {
                        MediaControlCommand::SetMetadata {
                            title,
                            artist,
                            album,
                            duration_secs,
                            cover_url,
                        } => {
                            let _ = controls.set_metadata(MediaMetadata {
                                title: title.as_deref(),
                                artist: artist.as_deref(),
                                album: album.as_deref(),
                                duration: duration_secs.map(Duration::from_secs),
                                cover_url: cover_url.as_deref(),
                            });
                        }
                        MediaControlCommand::SetPlayback(state) => {
                            let playback = match state {
                                PlaybackState::Playing { progress_secs } => {
                                    MediaPlayback::Playing {
                                        progress: progress_secs
                                            .map(|s| MediaPosition(Duration::from_secs_f64(s))),
                                    }
                                }
                                PlaybackState::Paused { progress_secs } => MediaPlayback::Paused {
                                    progress: progress_secs
                                        .map(|s| MediaPosition(Duration::from_secs_f64(s))),
                                },
                                PlaybackState::Stopped => MediaPlayback::Stopped,
                            };
                            let _ = controls.set_playback(playback);
                        }
                        MediaControlCommand::Shutdown => {
                            self.should_exit = true;
                        }
                    }
                }
            }

            if self.should_exit {
                event_loop.exit();
            }

            // Use a reasonable polling interval
            event_loop.set_control_flow(ControlFlow::wait_duration(Duration::from_millis(100)));
        }
    }

    let event_loop = EventLoop::new().map_err(|e| format!("Failed to create event loop: {}", e))?;

    let mut app = App {
        window: None,
        controls: None,
        cmd_rx,
        event_tx,
        should_exit: false,
    };

    debug!("Starting winit event loop on main thread");
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("Event loop error: {}", e))?;

    // Wait for app thread to finish
    match app_handle.join() {
        Ok(result) => result,
        Err(_) => Err("App thread panicked".to_string()),
    }
}
