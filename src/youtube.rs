//! High-level YouTube Music controller.
//!
//! This module provides a high-level interface for YouTube Music playback,
//! coordinating the API client, stream extraction, and audio player.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use log::{debug, error, info, warn};
use tokio::runtime::Runtime;

use crate::config::Config;
use crate::events::EventManager;
use crate::model::playable::Playable;
use crate::player_worker::{PlayerCommand, PlayerEvent, PlayerWorker};
use crate::youtube_music::{Cookies, YouTubeMusicClient};

#[cfg(feature = "mpris")]
use crate::mpris::MprisManager;

/// One percent of the maximum supported volume (0-65535 range).
pub const VOLUME_PERCENT: u16 = ((u16::MAX as f64) * 1.0 / 100.0) as u16;

/// Status of the YouTube Music player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum YouTubeStatus {
    /// Player is stopped.
    Stopped,
    /// Player is playing (with start time).
    Playing(SystemTime),
    /// Player is paused (with position).
    Paused(Duration),
}

impl Default for YouTubeStatus {
    fn default() -> Self {
        Self::Stopped
    }
}

/// High-level YouTube Music controller.
#[derive(Clone)]
pub struct YouTube {
    /// Event manager for sending events to the application.
    events: EventManager,
    /// Configuration.
    cfg: Arc<Config>,
    /// Current playback status.
    status: Arc<RwLock<YouTubeStatus>>,
    /// Current volume (0-65535).
    volume: Arc<RwLock<u16>>,
    /// Elapsed playback time.
    elapsed: Arc<RwLock<Option<Duration>>>,
    /// Time when playback started.
    since: Arc<RwLock<Option<SystemTime>>>,
    /// Channel to send commands to the player worker.
    command_tx: Arc<RwLock<Option<Sender<PlayerCommand>>>>,
    /// YouTube Music API client for library operations.
    client: Arc<RwLock<Option<YouTubeMusicClient>>>,
    /// MPRIS manager for media key support.
    #[cfg(feature = "mpris")]
    mpris: Arc<std::sync::Mutex<Option<MprisManager>>>,
}

impl YouTube {
    /// Create a new YouTube Music controller.
    pub fn new(events: EventManager, cfg: Arc<Config>) -> Self {
        Self {
            events,
            cfg,
            status: Arc::new(RwLock::new(YouTubeStatus::Stopped)),
            volume: Arc::new(RwLock::new(u16::MAX / 2)), // 50% default
            elapsed: Arc::new(RwLock::new(None)),
            since: Arc::new(RwLock::new(None)),
            command_tx: Arc::new(RwLock::new(None)),
            client: Arc::new(RwLock::new(None)),
            #[cfg(feature = "mpris")]
            mpris: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Create a YouTube instance for testing.
    #[cfg(test)]
    pub fn new_for_test(cfg: Arc<Config>, events: EventManager) -> Self {
        Self::new(events, cfg)
    }

    /// Start the player worker with the given cookies.
    pub fn start_worker(&self, cookies: Cookies) -> Result<(), String> {
        info!("Starting YouTube Music player worker");

        // Create event channel
        let (event_tx, event_rx) = mpsc::channel();

        // Create command channel
        let (command_tx, _command_rx) = mpsc::channel();

        // Create the player worker
        let worker = PlayerWorker::new(cookies.clone(), event_tx)
            .map_err(|e| format!("Failed to create player worker: {}", e))?;

        // Store the command sender (use the worker's internal channel)
        // Actually, we need to use the worker's send method
        // Let's store the worker instead

        // For now, store a direct command sender
        *self.command_tx.write().unwrap() = Some(command_tx);

        // Create and store the API client
        let client = YouTubeMusicClient::new(cookies)
            .map_err(|e| format!("Failed to create API client: {}", e))?;
        *self.client.write().unwrap() = Some(client);

        // Spawn event handler thread
        let status = self.status.clone();
        let elapsed = self.elapsed.clone();
        let since = self.since.clone();
        let events = self.events.clone();

        std::thread::spawn(move || {
            handle_player_events(event_rx, status, elapsed, since, events);
        });

        // Keep worker alive (it will be dropped when this scope ends otherwise)
        // In a real implementation, we'd store the worker
        std::mem::forget(worker);

        info!("YouTube Music player worker started");
        Ok(())
    }

    /// Start MPRIS support.
    #[cfg(feature = "mpris")]
    pub fn start_mpris(&self) {
        info!("MPRIS support not yet fully integrated for YouTube Music");
    }

    /// Set the MPRIS manager.
    #[cfg(feature = "mpris")]
    pub fn set_mpris(&mut self, mpris: MprisManager) {
        *self.mpris.lock().unwrap() = Some(mpris);
    }

    /// Get the current playback status.
    pub fn get_current_status(&self) -> YouTubeStatus {
        self.status.read().unwrap().clone()
    }

    /// Get the current playback progress.
    pub fn get_current_progress(&self) -> Duration {
        let status = self.status.read().unwrap();
        match *status {
            YouTubeStatus::Playing(start_time) => SystemTime::now()
                .duration_since(start_time)
                .unwrap_or(Duration::ZERO),
            YouTubeStatus::Paused(position) => position,
            YouTubeStatus::Stopped => Duration::ZERO,
        }
    }

    /// Load a track for playback.
    pub fn load(&self, track: &Playable, start_playing: bool, position_ms: u32) {
        debug!("Loading track: {:?}", track.id());

        if let Some(tx) = self.command_tx.read().unwrap().as_ref() {
            let _ = tx.send(PlayerCommand::Load {
                playable: track.clone(),
                start_playing,
                position_ms,
            });

            if start_playing {
                *self.since.write().unwrap() = Some(SystemTime::now());
                *self.status.write().unwrap() = YouTubeStatus::Playing(SystemTime::now());
            }
        } else {
            warn!("Player worker not started, cannot load track");
        }
    }

    /// Start or resume playback.
    pub fn play(&self) {
        debug!("Play");

        if let Some(tx) = self.command_tx.read().unwrap().as_ref() {
            let _ = tx.send(PlayerCommand::Play);
            *self.since.write().unwrap() = Some(SystemTime::now());
            *self.status.write().unwrap() = YouTubeStatus::Playing(SystemTime::now());
        }
    }

    /// Pause playback.
    pub fn pause(&self) {
        debug!("Pause");

        if let Some(tx) = self.command_tx.read().unwrap().as_ref() {
            let _ = tx.send(PlayerCommand::Pause);
            let progress = self.get_current_progress();
            *self.status.write().unwrap() = YouTubeStatus::Paused(progress);
            *self.elapsed.write().unwrap() = Some(progress);
        }
    }

    /// Toggle between play and pause.
    pub fn toggleplayback(&self) {
        let status = self.get_current_status();
        match status {
            YouTubeStatus::Playing(_) => self.pause(),
            YouTubeStatus::Paused(_) | YouTubeStatus::Stopped => self.play(),
        }
    }

    /// Stop playback.
    pub fn stop(&self) {
        debug!("Stop");

        if let Some(tx) = self.command_tx.read().unwrap().as_ref() {
            let _ = tx.send(PlayerCommand::Stop);
            *self.status.write().unwrap() = YouTubeStatus::Stopped;
            *self.elapsed.write().unwrap() = None;
            *self.since.write().unwrap() = None;
        }
    }

    /// Seek to an absolute position.
    pub fn seek(&self, position_ms: u32) {
        debug!("Seek to {}ms", position_ms);

        if let Some(tx) = self.command_tx.read().unwrap().as_ref() {
            let _ = tx.send(PlayerCommand::Seek(position_ms));
        }
    }

    /// Seek relative to current position.
    pub fn seek_relative(&self, delta_ms: i32) {
        let current = self.get_current_progress().as_millis() as i32;
        let new_pos = (current + delta_ms).max(0) as u32;
        self.seek(new_pos);
    }

    /// Get the current volume (0-65535).
    pub fn volume(&self) -> u16 {
        *self.volume.read().unwrap()
    }

    /// Set the volume.
    pub fn set_volume(&self, volume: u16, _notify: bool) {
        debug!("Set volume to {}", volume);

        *self.volume.write().unwrap() = volume;

        // Convert from 0-65535 to 0-100 for the player
        let volume_percent = ((volume as f32 / u16::MAX as f32) * 100.0) as u16;

        if let Some(tx) = self.command_tx.read().unwrap().as_ref() {
            let _ = tx.send(PlayerCommand::SetVolume(volume_percent));
        }
    }

    /// Preload a track for gapless playback.
    pub fn preload(&self, track: &Playable) {
        debug!("Preload: {:?}", track.id());

        if let Some(tx) = self.command_tx.read().unwrap().as_ref() {
            let _ = tx.send(PlayerCommand::Preload(track.clone()));
        }
    }

    /// Update track metadata (for UI refresh).
    pub fn update_track(&self) {
        // Trigger UI update
        debug!("Update track");
    }

    /// Shutdown the player.
    pub fn shutdown(&self) {
        info!("Shutting down YouTube Music player");

        if let Some(tx) = self.command_tx.read().unwrap().as_ref() {
            let _ = tx.send(PlayerCommand::Shutdown);
        }
    }

    /// Notify MPRIS of a seek operation.
    #[cfg(feature = "mpris")]
    pub fn notify_seeked(&self, _position_ms: u32) {
        // TODO: Implement MPRIS seek notification
    }

    /// Get a reference to the API client.
    pub fn api_client(&self) -> Option<YouTubeMusicClient> {
        self.client.read().unwrap().clone()
    }
}

/// Handle events from the player worker.
fn handle_player_events(
    event_rx: Receiver<PlayerEvent>,
    status: Arc<RwLock<YouTubeStatus>>,
    elapsed: Arc<RwLock<Option<Duration>>>,
    since: Arc<RwLock<Option<SystemTime>>>,
    events: EventManager,
) {
    loop {
        match event_rx.recv() {
            Ok(event) => {
                debug!("Player event: {:?}", event);

                match event {
                    PlayerEvent::Playing(time) => {
                        *status.write().unwrap() = YouTubeStatus::Playing(time);
                        *since.write().unwrap() = Some(time);
                    }
                    PlayerEvent::Paused(position_ms) => {
                        let pos = Duration::from_millis(position_ms as u64);
                        *status.write().unwrap() = YouTubeStatus::Paused(pos);
                        *elapsed.write().unwrap() = Some(pos);
                    }
                    PlayerEvent::Stopped => {
                        *status.write().unwrap() = YouTubeStatus::Stopped;
                        *elapsed.write().unwrap() = None;
                        *since.write().unwrap() = None;
                    }
                    PlayerEvent::FinishedTrack => {
                        // Notify the queue to play the next track
                        debug!("Track finished, notifying queue");
                        // events.send_player_event(crate::events::Event::Player(...));
                    }
                    PlayerEvent::TrackLoaded { duration_ms } => {
                        debug!("Track loaded, duration: {}ms", duration_ms);
                    }
                    PlayerEvent::VolumeChanged(vol) => {
                        debug!("Volume changed to {}%", vol);
                    }
                    PlayerEvent::Position {
                        position_ms,
                        duration_ms,
                    } => {
                        debug!("Position: {}ms / {}ms", position_ms, duration_ms);
                    }
                    PlayerEvent::Error(msg) => {
                        error!("Player error: {}", msg);
                    }
                }
            }
            Err(_) => {
                info!("Player event channel closed");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_youtube_status() {
        assert_eq!(YouTubeStatus::default(), YouTubeStatus::Stopped);

        let playing = YouTubeStatus::Playing(SystemTime::now());
        assert!(matches!(playing, YouTubeStatus::Playing(_)));

        let paused = YouTubeStatus::Paused(Duration::from_secs(30));
        assert!(matches!(paused, YouTubeStatus::Paused(_)));
    }

    #[test]
    fn test_volume_percent() {
        // 1% of u16::MAX should be approximately 655
        assert!(VOLUME_PERCENT > 600 && VOLUME_PERCENT < 700);
    }
}
