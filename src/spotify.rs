//! Playback controller wrapping YouTube Music functionality.

use std::io::Write;
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::config;
use crate::events::EventManager;
use crate::model::playable::Playable;
use crate::player::Player;
use crate::spotify_api::WebApi;
use crate::youtube_music::{AudioQuality, Cookies, YouTubeMusicClient, get_stream_url};

#[cfg(feature = "mpris")]
use crate::mpris::MprisManager;

/// Debug log to file
fn dlog(msg: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/ncytm_debug.log")
    {
        let _ = writeln!(
            f,
            "[{}] {}",
            chrono::Local::now().format("%H:%M:%S%.3f"),
            msg
        );
    }
}

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
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() >= 2 {
            match parts[1].to_lowercase().as_str() {
                "track" | "video" => Ok(Self::Track),
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
}

/// Stub credentials type for backward compatibility.
#[derive(Clone, Debug, Default)]
pub struct Credentials {}

/// Commands sent to the player thread.
#[derive(Debug)]
enum PlayerCommand {
    Load {
        video_id: String,
        start_playing: bool,
    },
    Play,
    Pause,
    Stop,
    SetVolume(f32),
    Shutdown,
}

/// Playback controller.
#[derive(Clone)]
pub struct Spotify {
    events: EventManager,
    #[cfg(feature = "mpris")]
    mpris: Arc<std::sync::Mutex<Option<MprisManager>>>,
    #[allow(dead_code)]
    cfg: Arc<config::Config>,
    status: Arc<RwLock<PlayerEvent>>,
    pub api: WebApi,
    elapsed: Arc<RwLock<Option<Duration>>>,
    since: Arc<RwLock<Option<SystemTime>>>,
    volume: Arc<RwLock<u16>>,
    cookies: Arc<RwLock<Option<Cookies>>>,
    command_tx: Arc<RwLock<Option<Sender<PlayerCommand>>>>,
    current_track: Arc<RwLock<Option<Playable>>>,
}

impl Spotify {
    #[cfg(test)]
    pub fn new_for_test(cfg: Arc<config::Config>, events: EventManager) -> Self {
        Self {
            events,
            #[cfg(feature = "mpris")]
            mpris: Default::default(),
            cfg,
            status: Arc::new(RwLock::new(PlayerEvent::Stopped)),
            api: WebApi::new(),
            elapsed: Arc::new(RwLock::new(None)),
            since: Arc::new(RwLock::new(None)),
            volume: Arc::new(RwLock::new(u16::MAX / 2)),
            cookies: Arc::new(RwLock::new(None)),
            command_tx: Arc::new(RwLock::new(None)),
            current_track: Arc::new(RwLock::new(None)),
        }
    }

    pub fn new(
        events: EventManager,
        _credentials: Credentials,
        cfg: Arc<config::Config>,
    ) -> Result<Self, String> {
        // Clear debug log
        let _ = std::fs::write("/tmp/ncytm_debug.log", "");
        dlog("Creating YouTube Music playback controller");

        Ok(Self {
            events,
            #[cfg(feature = "mpris")]
            mpris: Default::default(),
            cfg,
            status: Arc::new(RwLock::new(PlayerEvent::Stopped)),
            api: WebApi::new(),
            elapsed: Arc::new(RwLock::new(None)),
            since: Arc::new(RwLock::new(None)),
            volume: Arc::new(RwLock::new(u16::MAX / 2)),
            cookies: Arc::new(RwLock::new(None)),
            command_tx: Arc::new(RwLock::new(None)),
            current_track: Arc::new(RwLock::new(None)),
        })
    }

    pub fn set_cookies(&self, cookies: Cookies) {
        *self.cookies.write().unwrap() = Some(cookies);
    }

    pub fn start_worker(&self, _credentials: Option<Credentials>) -> Result<(), String> {
        dlog("Starting player worker thread");

        let cookies = self
            .cookies
            .read()
            .unwrap()
            .clone()
            .ok_or("No cookies set")?;

        let (command_tx, command_rx) = mpsc::channel();
        *self.command_tx.write().unwrap() = Some(command_tx);

        let status = self.status.clone();
        let since = self.since.clone();
        let events = self.events.clone();

        thread::spawn(move || {
            run_player_thread(cookies, command_rx, status, since, events);
        });

        dlog("Player worker thread started");
        Ok(())
    }

    #[cfg(feature = "mpris")]
    pub fn start_mpris(&self) {
        info!("MPRIS support enabled");
    }

    #[cfg(feature = "mpris")]
    pub fn set_mpris(&mut self, mpris: MprisManager) {
        *self.mpris.lock().unwrap() = Some(mpris);
    }

    pub fn get_current_status(&self) -> PlayerEvent {
        self.status.read().unwrap().clone()
    }

    pub fn get_current_progress(&self) -> Duration {
        let status = self.status.read().unwrap().clone();
        match status {
            PlayerEvent::Playing(start) => SystemTime::now()
                .duration_since(start)
                .unwrap_or(Duration::ZERO),
            PlayerEvent::Paused(elapsed) => elapsed,
            _ => Duration::ZERO,
        }
    }

    pub fn load(&self, track: &Playable, start_playing: bool, _position_ms: u32) {
        let video_id = match track.id() {
            Some(id) => id.to_string(),
            None => {
                dlog("Track has no video ID!");
                return;
            }
        };

        dlog(&format!("Loading track: {}", video_id));
        *self.current_track.write().unwrap() = Some(track.clone());

        if let Some(ref tx) = *self.command_tx.read().unwrap() {
            dlog("Sending load command to player thread");
            let _ = tx.send(PlayerCommand::Load {
                video_id,
                start_playing,
            });
        } else {
            dlog("Player thread not started!");
        }
    }

    pub fn update_track(&self) {
        self.events.trigger();
    }

    pub fn play(&self) {
        dlog("Play");
        if let Some(ref tx) = *self.command_tx.read().unwrap() {
            let _ = tx.send(PlayerCommand::Play);
            *self.status.write().unwrap() = PlayerEvent::Playing(SystemTime::now());
            *self.since.write().unwrap() = Some(SystemTime::now());
            self.events.trigger();
        }
    }

    pub fn toggleplayback(&self) {
        let status = self.get_current_status();
        match status {
            PlayerEvent::Playing(_) => self.pause(),
            _ => self.play(),
        }
    }

    pub fn pause(&self) {
        dlog("Pause");
        if let Some(ref tx) = *self.command_tx.read().unwrap() {
            let _ = tx.send(PlayerCommand::Pause);
            let progress = self.get_current_progress();
            *self.status.write().unwrap() = PlayerEvent::Paused(progress);
            *self.elapsed.write().unwrap() = Some(progress);
            self.events.trigger();
        }
    }

    pub fn stop(&self) {
        dlog("Stop");
        if let Some(ref tx) = *self.command_tx.read().unwrap() {
            let _ = tx.send(PlayerCommand::Stop);
            *self.status.write().unwrap() = PlayerEvent::Stopped;
            *self.elapsed.write().unwrap() = None;
            *self.since.write().unwrap() = None;
            self.events.trigger();
        }
    }

    pub fn seek(&self, _position_ms: u32) {
        dlog("Seek not yet implemented");
    }

    pub fn seek_relative(&self, _delta_ms: i32) {
        dlog("Seek relative not yet implemented");
    }

    pub fn volume(&self) -> u16 {
        *self.volume.read().unwrap()
    }

    pub fn set_volume(&self, volume: u16, _notify: bool) {
        *self.volume.write().unwrap() = volume;
        let volume_f32 = volume as f32 / u16::MAX as f32;
        if let Some(ref tx) = *self.command_tx.read().unwrap() {
            let _ = tx.send(PlayerCommand::SetVolume(volume_f32));
        }
    }

    pub fn shutdown(&self) {
        dlog("Shutting down player");
        if let Some(ref tx) = *self.command_tx.read().unwrap() {
            let _ = tx.send(PlayerCommand::Shutdown);
        }
    }

    #[cfg(feature = "mpris")]
    pub fn notify_seeked(&self, _position_ms: u32) {}
}

fn run_player_thread(
    cookies: Cookies,
    command_rx: Receiver<PlayerCommand>,
    status: Arc<RwLock<PlayerEvent>>,
    since: Arc<RwLock<Option<SystemTime>>>,
    events: EventManager,
) {
    dlog("Player thread starting");

    let mut player = match Player::new() {
        Ok(p) => {
            dlog("Audio player created successfully");
            p
        }
        Err(e) => {
            dlog(&format!("Failed to create audio player: {:?}", e));
            return;
        }
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            dlog(&format!("Failed to create tokio runtime: {}", e));
            return;
        }
    };

    let client = match YouTubeMusicClient::new(cookies) {
        Ok(c) => {
            dlog("YouTube Music client created");
            c
        }
        Err(e) => {
            dlog(&format!("Failed to create client: {:?}", e));
            return;
        }
    };

    dlog("Player thread ready, waiting for commands");

    loop {
        match command_rx.recv() {
            Ok(cmd) => {
                dlog(&format!("Received command: {:?}", cmd));
                match cmd {
                    PlayerCommand::Load {
                        video_id,
                        start_playing,
                    } => {
                        dlog(&format!("Fetching stream URL for: {}", video_id));

                        let stream_result = rt.block_on(async {
                            get_stream_url(&client, &video_id, AudioQuality::High).await
                        });

                        match stream_result {
                            Ok(stream_info) => {
                                dlog(&format!("Got stream URL, mime: {}", stream_info.mime_type));
                                dlog(&format!(
                                    "URL: {}...",
                                    &stream_info.url[..stream_info.url.len().min(100)]
                                ));

                                dlog("Calling player.load_url...");
                                match player.load_url(&stream_info.url, start_playing) {
                                    Ok(()) => {
                                        dlog("Track loaded into player successfully!");
                                        if start_playing {
                                            *status.write().unwrap() =
                                                PlayerEvent::Playing(SystemTime::now());
                                            *since.write().unwrap() = Some(SystemTime::now());
                                            dlog("Status set to Playing");
                                        }
                                        events.trigger();
                                        dlog("Events triggered");
                                    }
                                    Err(e) => {
                                        dlog(&format!("Failed to load into player: {:?}", e));
                                    }
                                }
                            }
                            Err(e) => {
                                dlog(&format!("Failed to get stream URL: {:?}", e));
                            }
                        }
                    }
                    PlayerCommand::Play => {
                        dlog("Executing play command");
                        player.play();
                    }
                    PlayerCommand::Pause => {
                        dlog("Executing pause command");
                        player.pause();
                    }
                    PlayerCommand::Stop => {
                        dlog("Executing stop command");
                        player.stop();
                    }
                    PlayerCommand::SetVolume(vol) => {
                        player.set_volume(vol);
                    }
                    PlayerCommand::Shutdown => {
                        dlog("Shutting down player thread");
                        player.stop();
                        break;
                    }
                }
            }
            Err(_) => {
                dlog("Command channel closed");
                break;
            }
        }
    }
}
