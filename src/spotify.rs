//! Stub module for Spotify functionality.
//! This will be replaced with YouTube Music implementation.

use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use log::info;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::config;
use crate::events::EventManager;
use crate::model::playable::Playable;
#[cfg(feature = "mpris")]
use crate::mpris::MprisManager;
use crate::spotify_api::WebApi;
use crate::spotify_worker::WorkerCommand;

/// One percent of the maximum supported volume.
pub const VOLUME_PERCENT: u16 = ((u16::MAX as f64) * 1.0 / 100.0) as u16;

/// URI types for music items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UriType {
    Album,
    Artist,
    Episode,
    Playlist,
    Show,
    Track,
}

impl FromStr for UriType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Parse URIs like "spotify:track:xxx" or "youtube:video:xxx"
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() >= 2 {
            match parts[1].to_lowercase().as_str() {
                "track" | "video" => Ok(Self::Track),
                "album" | "playlist" if parts.len() > 2 && parts[2].contains("album") => {
                    Ok(Self::Album)
                }
                "album" => Ok(Self::Album),
                "artist" | "channel" => Ok(Self::Artist),
                "playlist" => Ok(Self::Playlist),
                "show" => Ok(Self::Show),
                "episode" => Ok(Self::Episode),
                _ => Err(format!("Unknown URI type: {}", s)),
            }
        } else {
            Err(format!("Invalid URI format: {}", s))
        }
    }
}

/// Events sent by the Player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum PlayerEvent {
    Playing(SystemTime),
    Paused(Duration),
    Stopped,
    FinishedTrack,
}

/// Stub credentials type.
#[derive(Clone, Debug)]
pub struct Credentials {
    pub username: Option<String>,
}

impl Default for Credentials {
    fn default() -> Self {
        Self { username: None }
    }
}

/// Wrapper around playback functionality.
/// Currently stubbed - will be replaced with YouTube Music implementation.
#[derive(Clone)]
pub struct Spotify {
    events: EventManager,
    #[cfg(feature = "mpris")]
    mpris: Arc<std::sync::Mutex<Option<MprisManager>>>,
    credentials: Credentials,
    cfg: Arc<config::Config>,
    status: Arc<RwLock<PlayerEvent>>,
    pub api: WebApi,
    elapsed: Arc<RwLock<Option<Duration>>>,
    since: Arc<RwLock<Option<SystemTime>>>,
    channel: Arc<RwLock<Option<mpsc::UnboundedSender<WorkerCommand>>>>,
}

impl Spotify {
    /// Create a Spotify instance for testing without full initialization.
    #[cfg(test)]
    pub fn new_for_test(cfg: Arc<config::Config>, events: EventManager) -> Self {
        Self {
            events,
            #[cfg(feature = "mpris")]
            mpris: Default::default(),
            credentials: Credentials::default(),
            cfg,
            status: Arc::new(RwLock::new(PlayerEvent::Stopped)),
            api: WebApi::new(),
            elapsed: Arc::new(RwLock::new(None)),
            since: Arc::new(RwLock::new(None)),
            channel: Arc::new(RwLock::new(None)),
        }
    }

    pub fn new(
        events: EventManager,
        credentials: Credentials,
        cfg: Arc<config::Config>,
    ) -> Result<Self, String> {
        info!("Creating Spotify stub (will be replaced with YouTube Music)");
        Ok(Self {
            events,
            #[cfg(feature = "mpris")]
            mpris: Default::default(),
            credentials,
            cfg,
            status: Arc::new(RwLock::new(PlayerEvent::Stopped)),
            api: WebApi::new(),
            elapsed: Arc::new(RwLock::new(None)),
            since: Arc::new(RwLock::new(None)),
            channel: Arc::new(RwLock::new(None)),
        })
    }

    pub fn test_credentials(
        _cfg: &config::Config,
        _credentials: Credentials,
    ) -> Result<(), String> {
        // Stub: always succeed
        Ok(())
    }

    pub fn start_worker(&self, _credentials: Option<Credentials>) -> Result<(), String> {
        info!("Worker start stubbed");
        Ok(())
    }

    #[cfg(feature = "mpris")]
    pub fn start_mpris(&self) {
        info!("MPRIS start stubbed");
    }

    #[cfg(feature = "mpris")]
    pub fn set_mpris(&mut self, mpris: MprisManager) {
        *self.mpris.lock().unwrap() = Some(mpris);
    }

    pub fn update_status(&self, status: PlayerEvent) {
        *self.status.write().unwrap() = status;
    }

    pub fn get_current_status(&self) -> PlayerEvent {
        self.status.read().unwrap().clone()
    }

    pub fn get_current_progress(&self) -> Duration {
        Duration::from_secs(0)
    }

    pub fn load(&self, _track: &Playable, _start_playing: bool, _position_ms: u32) {
        info!("Load stubbed");
    }

    pub fn update_track(&self) {
        info!("Update track stubbed");
    }

    pub fn play(&self) {
        info!("Play stubbed");
    }

    pub fn toggleplayback(&self) {
        info!("Toggle playback stubbed");
    }

    pub fn pause(&self) {
        info!("Pause stubbed");
    }

    pub fn stop(&self) {
        info!("Stop stubbed");
    }

    pub fn seek(&self, _position_ms: u32) {
        info!("Seek stubbed");
    }

    pub fn seek_relative(&self, _delta_ms: i32) {
        info!("Seek relative stubbed");
    }

    pub fn volume(&self) -> u16 {
        u16::MAX / 2
    }

    pub fn set_volume(&self, _volume: u16, _notify: bool) {
        info!("Set volume stubbed");
    }

    pub fn preload(&self, _track: &Playable) {
        info!("Preload stubbed");
    }

    pub fn shutdown(&self) {
        info!("Shutdown stubbed");
    }

    #[cfg(feature = "mpris")]
    pub fn notify_seeked(&self, _position_ms: u32) {
        info!("Notify seeked stubbed");
    }
}
