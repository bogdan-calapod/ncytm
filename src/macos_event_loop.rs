//! macOS main thread event loop for media control support.
//!
//! On macOS, media controls require an AppDelegate/winit event loop running on the main thread.
//! This module provides the infrastructure to run the app's TUI in a worker thread while
//! keeping the winit event loop on main.

use std::sync::mpsc;
use std::thread;

use log::{debug, warn};

/// Messages from the app to the media control event loop
enum MediaControlCommand {
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
    /// Reclaim the macOS "Now Playing" slot: re-register the remote command
    /// handlers and force a real `Paused` -> `Playing` state transition with
    /// fresh metadata, which is what prompts macOS to re-elect ncytm as the
    /// current Now Playing app. Carries the current track's metadata so the
    /// whole sequence can run atomically on the main thread.
    Reclaim {
        title: Option<String>,
        artist: Option<String>,
        album: Option<String>,
        duration_secs: Option<u64>,
        cover_url: Option<String>,
        progress_secs: Option<f64>,
    },
}

/// Playback state for media controls
#[derive(Clone, Debug)]
pub enum PlaybackState {
    Playing { progress_secs: Option<f64> },
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

    /// Reclaim the macOS "Now Playing" slot for ncytm.
    ///
    /// Re-registers the remote command handlers and forces a real
    /// `Paused` -> `Playing` transition with the given metadata, which prompts
    /// macOS to re-elect ncytm as the current Now Playing app.
    #[allow(clippy::too_many_arguments)]
    pub fn reclaim(
        &self,
        title: Option<&str>,
        artist: Option<&str>,
        album: Option<&str>,
        duration_secs: Option<u64>,
        cover_url: Option<&str>,
        progress_secs: Option<f64>,
    ) {
        let _ = self.tx.send(MediaControlCommand::Reclaim {
            title: title.map(String::from),
            artist: artist.map(String::from),
            album: album.map(String::from),
            duration_secs,
            cover_url: cover_url.map(String::from),
            progress_secs,
        });
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

    /// Build the closure that forwards souvlaki media control events to the app.
    ///
    /// Shared between the initial `attach` and the `Reclaim` re-`attach`, so both
    /// register identical command handlers.
    fn make_event_handler(
        event_tx: mpsc::Sender<MediaControlEvent>,
    ) -> impl Fn(souvlaki::MediaControlEvent) + Send + 'static {
        move |e| {
            let event = match e {
                souvlaki::MediaControlEvent::Play => MediaControlEvent::Play,
                souvlaki::MediaControlEvent::Pause => MediaControlEvent::Pause,
                souvlaki::MediaControlEvent::Toggle => MediaControlEvent::Toggle,
                souvlaki::MediaControlEvent::Next => MediaControlEvent::Next,
                souvlaki::MediaControlEvent::Previous => MediaControlEvent::Previous,
                souvlaki::MediaControlEvent::Stop => MediaControlEvent::Stop,
                souvlaki::MediaControlEvent::Seek(souvlaki::SeekDirection::Forward) => {
                    MediaControlEvent::SeekForward
                }
                souvlaki::MediaControlEvent::Seek(souvlaki::SeekDirection::Backward) => {
                    MediaControlEvent::SeekBackward
                }
                souvlaki::MediaControlEvent::SetPosition(souvlaki::MediaPosition(dur)) => {
                    MediaControlEvent::SetPosition(dur.as_secs_f64())
                }
                _ => return,
            };
            let _ = event_tx.send(event);
        }
    }

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

                            if let Err(e) =
                                controls.attach(make_event_handler(self.event_tx.clone()))
                            {
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
            loop {
                match self.cmd_rx.try_recv() {
                    Ok(cmd) => {
                        let event_tx = self.event_tx.clone();
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
                                    let PlaybackState::Playing { progress_secs } = state;
                                    let playback = MediaPlayback::Playing {
                                        progress: progress_secs
                                            .map(|s| MediaPosition(Duration::from_secs_f64(s))),
                                    };
                                    let _ = controls.set_playback(playback);
                                }
                                MediaControlCommand::Reclaim {
                                    title,
                                    artist,
                                    album,
                                    duration_secs,
                                    cover_url,
                                    progress_secs,
                                } => {
                                    // Re-register the remote command handlers so
                                    // macOS re-elects ncytm as the Now Playing
                                    // app. detach() first to avoid stacking
                                    // duplicate handlers across many tracks.
                                    let _ = controls.detach();
                                    if let Err(e) = controls.attach(make_event_handler(event_tx)) {
                                        warn!("winit: failed to re-attach handlers: {:?}", e);
                                    }

                                    // Set metadata fresh.
                                    let _ = controls.set_metadata(MediaMetadata {
                                        title: title.as_deref(),
                                        artist: artist.as_deref(),
                                        album: album.as_deref(),
                                        duration: duration_secs.map(Duration::from_secs),
                                        cover_url: cover_url.as_deref(),
                                    });

                                    // Force a real Stopped -> Paused -> Playing
                                    // transition. macOS only re-elects the Now
                                    // Playing app on an actual state change;
                                    // re-setting Playing when already Playing is a
                                    // no-op. Small sleeps ensure the AppKit run
                                    // loop registers each distinct transition
                                    // rather than coalescing them.
                                    let progress = progress_secs
                                        .map(|s| MediaPosition(Duration::from_secs_f64(s)));
                                    let _ = controls.set_playback(MediaPlayback::Stopped);
                                    std::thread::sleep(Duration::from_millis(20));
                                    let _ =
                                        controls.set_playback(MediaPlayback::Paused { progress });
                                    std::thread::sleep(Duration::from_millis(20));
                                    let _ =
                                        controls.set_playback(MediaPlayback::Playing { progress });
                                    debug!("winit: reclaimed media focus (re-attach + transition)");
                                }
                            }
                        }
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        debug!("App thread exited, stopping winit event loop");
                        event_loop.exit();
                        return;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                }
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
