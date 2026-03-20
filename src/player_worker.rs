//! Player worker thread for YouTube Music playback.
//!
//! This module manages audio playback in a background thread, communicating
//! with the main application via channels.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use log::{debug, error, info, warn};
use tokio::runtime::Runtime;

use crate::model::playable::Playable;
use crate::player::{Player, PlayerError, PlayerState};
use crate::youtube_music::{get_stream_url, AudioQuality, Cookies, StreamError, YouTubeMusicClient};

/// Commands that can be sent to the player worker.
#[derive(Debug)]
pub enum PlayerCommand {
    /// Load a track and optionally start playing.
    Load {
        playable: Playable,
        start_playing: bool,
        position_ms: u32,
    },
    /// Start/resume playback.
    Play,
    /// Pause playback.
    Pause,
    /// Stop playback.
    Stop,
    /// Seek to a position in milliseconds.
    Seek(u32),
    /// Set volume (0-100).
    SetVolume(u16),
    /// Preload a track for gapless playback.
    Preload(Playable),
    /// Request current playback status.
    RequestStatus,
    /// Shutdown the worker thread.
    Shutdown,
}

/// Events emitted by the player worker.
#[derive(Debug, Clone)]
pub enum PlayerEvent {
    /// Playback started at the given time.
    Playing(SystemTime),
    /// Playback paused at the given position (ms).
    Paused(u32),
    /// Playback stopped.
    Stopped,
    /// Current track finished playing.
    FinishedTrack,
    /// Track loaded and ready to play.
    TrackLoaded {
        duration_ms: u32,
    },
    /// Volume changed.
    VolumeChanged(u16),
    /// Playback position update.
    Position {
        position_ms: u32,
        duration_ms: u32,
    },
    /// An error occurred.
    Error(String),
}

/// Player worker that runs in a background thread.
pub struct PlayerWorker {
    /// Channel to send commands to the worker.
    command_tx: Sender<PlayerCommand>,
    /// Handle to the worker thread.
    thread_handle: Option<JoinHandle<()>>,
}

impl PlayerWorker {
    /// Create a new player worker with the given YouTube Music client.
    ///
    /// # Arguments
    ///
    /// * `cookies` - YouTube Music cookies for authentication
    /// * `event_tx` - Channel to send player events to the main application
    pub fn new(cookies: Cookies, event_tx: Sender<PlayerEvent>) -> Result<Self, PlayerError> {
        let (command_tx, command_rx) = mpsc::channel();

        // Spawn the worker thread
        let thread_handle = thread::spawn(move || {
            run_worker(cookies, command_rx, event_tx);
        });

        Ok(Self {
            command_tx,
            thread_handle: Some(thread_handle),
        })
    }

    /// Send a command to the player worker.
    pub fn send(&self, command: PlayerCommand) -> Result<(), mpsc::SendError<PlayerCommand>> {
        self.command_tx.send(command)
    }

    /// Load and play a track.
    pub fn load(&self, playable: Playable, start_playing: bool, position_ms: u32) {
        let _ = self.send(PlayerCommand::Load {
            playable,
            start_playing,
            position_ms,
        });
    }

    /// Start/resume playback.
    pub fn play(&self) {
        let _ = self.send(PlayerCommand::Play);
    }

    /// Pause playback.
    pub fn pause(&self) {
        let _ = self.send(PlayerCommand::Pause);
    }

    /// Stop playback.
    pub fn stop(&self) {
        let _ = self.send(PlayerCommand::Stop);
    }

    /// Seek to a position.
    pub fn seek(&self, position_ms: u32) {
        let _ = self.send(PlayerCommand::Seek(position_ms));
    }

    /// Set volume (0-100).
    pub fn set_volume(&self, volume: u16) {
        let _ = self.send(PlayerCommand::SetVolume(volume));
    }

    /// Preload a track for gapless playback.
    pub fn preload(&self, playable: Playable) {
        let _ = self.send(PlayerCommand::Preload(playable));
    }

    /// Shutdown the worker.
    pub fn shutdown(&self) {
        let _ = self.send(PlayerCommand::Shutdown);
    }
}

impl Drop for PlayerWorker {
    fn drop(&mut self) {
        // Send shutdown command
        let _ = self.command_tx.send(PlayerCommand::Shutdown);

        // Wait for thread to finish
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Run the player worker loop.
fn run_worker(cookies: Cookies, command_rx: Receiver<PlayerCommand>, event_tx: Sender<PlayerEvent>) {
    info!("Player worker started");

    // Create tokio runtime for async operations
    let runtime = match Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            error!("Failed to create tokio runtime: {}", e);
            let _ = event_tx.send(PlayerEvent::Error(format!("Runtime error: {}", e)));
            return;
        }
    };

    // Create YouTube Music client
    let client = match YouTubeMusicClient::new(cookies) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create YouTube Music client: {}", e);
            let _ = event_tx.send(PlayerEvent::Error(format!("Client error: {}", e)));
            return;
        }
    };

    // Create audio player
    let mut player = match Player::new() {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to create audio player: {}", e);
            let _ = event_tx.send(PlayerEvent::Error(format!("Player error: {}", e)));
            return;
        }
    };

    // Current volume (0-100)
    let mut current_volume: u16 = 100;

    loop {
        // Check for finished playback
        if player.state() == PlayerState::Playing && player.is_finished() {
            debug!("Track finished");
            let _ = event_tx.send(PlayerEvent::FinishedTrack);
        }

        // Process commands with a timeout
        match command_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(command) => {
                match command {
                    PlayerCommand::Load {
                        playable,
                        start_playing,
                        position_ms,
                    } => {
                        debug!("Loading track: {:?}", playable.id());

                        // Get stream URL
                        let video_id = match playable.id() {
                            Some(id) => id,
                            None => {
                                let _ = event_tx.send(PlayerEvent::Error(
                                    "Track has no video ID".to_string(),
                                ));
                                continue;
                            }
                        };

                        let stream_result = runtime.block_on(async {
                            get_stream_url(&client, &video_id, AudioQuality::High).await
                        });

                        match stream_result {
                            Ok(stream_info) => {
                                info!(
                                    "Got stream URL: {} ({}kbps)",
                                    &stream_info.url[..stream_info.url.len().min(50)],
                                    stream_info.bitrate / 1000
                                );

                                // Load the audio
                                match player.load_url(&stream_info.url, start_playing) {
                                    Ok(()) => {
                                        let duration_ms =
                                            stream_info.duration_seconds.unwrap_or(0) * 1000;
                                        let _ = event_tx.send(PlayerEvent::TrackLoaded {
                                            duration_ms,
                                        });

                                        if start_playing {
                                            let _ = event_tx.send(PlayerEvent::Playing(
                                                SystemTime::now(),
                                            ));
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to load audio: {}", e);
                                        let _ = event_tx.send(PlayerEvent::Error(format!(
                                            "Load error: {}",
                                            e
                                        )));
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to get stream URL: {}", e);
                                let _ = event_tx.send(PlayerEvent::Error(format!(
                                    "Stream error: {}",
                                    e
                                )));
                            }
                        }
                    }

                    PlayerCommand::Play => {
                        debug!("Play command received");
                        player.play();
                        let _ = event_tx.send(PlayerEvent::Playing(SystemTime::now()));
                    }

                    PlayerCommand::Pause => {
                        debug!("Pause command received");
                        player.pause();
                        let position_ms = player.position().as_millis() as u32;
                        let _ = event_tx.send(PlayerEvent::Paused(position_ms));
                    }

                    PlayerCommand::Stop => {
                        debug!("Stop command received");
                        player.stop();
                        let _ = event_tx.send(PlayerEvent::Stopped);
                    }

                    PlayerCommand::Seek(position_ms) => {
                        debug!("Seek command received: {}ms", position_ms);
                        let _ = player.seek(Duration::from_millis(position_ms as u64));
                    }

                    PlayerCommand::SetVolume(volume) => {
                        debug!("Volume command received: {}", volume);
                        current_volume = volume.min(100);
                        player.set_volume(current_volume as f32 / 100.0);
                        let _ = event_tx.send(PlayerEvent::VolumeChanged(current_volume));
                    }

                    PlayerCommand::Preload(playable) => {
                        debug!("Preload command received: {:?}", playable.id());
                        // TODO: Implement preloading for gapless playback
                    }

                    PlayerCommand::RequestStatus => {
                        let position_ms = player.position().as_millis() as u32;
                        let duration_ms = player.duration().as_millis() as u32;
                        let _ = event_tx.send(PlayerEvent::Position {
                            position_ms,
                            duration_ms,
                        });
                    }

                    PlayerCommand::Shutdown => {
                        info!("Shutdown command received");
                        player.stop();
                        break;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // No command received, continue loop
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                info!("Command channel disconnected, shutting down");
                break;
            }
        }
    }

    info!("Player worker stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_command_debug() {
        let cmd = PlayerCommand::Play;
        assert!(format!("{:?}", cmd).contains("Play"));

        let cmd = PlayerCommand::SetVolume(50);
        assert!(format!("{:?}", cmd).contains("50"));
    }

    #[test]
    fn test_player_event_clone() {
        let event = PlayerEvent::Playing(SystemTime::now());
        let _cloned = event.clone();

        let event = PlayerEvent::VolumeChanged(75);
        let cloned = event.clone();
        assert!(matches!(cloned, PlayerEvent::VolumeChanged(75)));
    }

    #[test]
    fn test_player_event_debug() {
        let event = PlayerEvent::Stopped;
        assert!(format!("{:?}", event).contains("Stopped"));

        let event = PlayerEvent::Error("test error".to_string());
        assert!(format!("{:?}", event).contains("test error"));
    }
}
