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
use crate::youtube_music::stream::StreamInfo;
use crate::youtube_music::{
    AudioQuality, Cookies, YouTubeMusicClient, api::player::get_video_duration, get_stream_url,
};

#[cfg(feature = "mpris")]
use crate::mpris::MprisManager;

/// Produce a short, user-facing reason string from a stream error.
fn stream_error_message(err: &crate::youtube_music::stream::StreamError) -> String {
    use crate::youtube_music::stream::StreamError;
    match err {
        StreamError::ApiError { message } => message.clone(),
        StreamError::NotPlayable { reason } => format!("video not playable: {reason}"),
        StreamError::VideoNotFound { video_id } => format!("video not found: {video_id}"),
        StreamError::NoAudioStreams => "no audio streams available".to_string(),
        other => other.to_string(),
    }
}

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
    /// Track is being fetched/buffered.
    Loading,
    Playing(SystemTime),
    Paused(Duration),
    Stopped,
    /// Playback failed (e.g. yt-dlp error). Carries a short reason for display.
    FailedToPlay(String),
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
    Restart,
    Shutdown,
    /// Pass the queue's track storage so the player thread can update
    /// durations when yt-dlp resolves them.
    SetQueue(Arc<RwLock<Vec<Playable>>>),
    /// Eagerly resolve all 0-duration tracks in the queue using the YouTube
    /// player API (lightweight, no yt-dlp). The player thread spawns
    /// non-blocking tasks and updates the queue as results arrive.
    ResolveDurations,
    /// Pre-download stream URLs for the given video IDs in the background.
    /// Skips videos that already have a temp file on disk.
    PreloadStreamUrls(Vec<String>),
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
    /// Flag indicating the current track has finished playing.
    track_finished: Arc<std::sync::atomic::AtomicBool>,
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
            track_finished: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
            track_finished: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    pub fn set_cookies(&mut self, cookies: Cookies) {
        *self.cookies.write().unwrap() = Some(cookies.clone());
        // Initialize the WebApi client with cookies
        if let Err(e) = self.api.init_from_cookies(cookies) {
            log::error!("Failed to initialize WebApi client: {}", e);
        }
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
        let track_finished = self.track_finished.clone();
        let current_track = self.current_track.clone();

        thread::spawn(move || {
            run_player_thread(
                cookies,
                command_rx,
                status,
                since,
                events,
                track_finished,
                current_track,
            );
        });

        dlog("Player worker thread started");
        Ok(())
    }

    /// Check if the current track has finished playing and reset the flag.
    /// Returns `true` if the track finished, `false` otherwise.
    pub fn take_track_finished(&self) -> bool {
        self.track_finished
            .swap(false, std::sync::atomic::Ordering::SeqCst)
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

        // Pause current playback before loading new track
        if let Some(ref tx) = *self.command_tx.read().unwrap() {
            if let Err(e) = tx.send(PlayerCommand::Pause) {
                dlog(&format!("Failed to send Pause: {:?}", e));
            }
        } else {
            dlog("Player thread not running — skipping Pause");
        }

        *self.current_track.write().unwrap() = Some(track.clone());

        // Set loading state immediately so UI shows spinner
        *self.status.write().unwrap() = PlayerEvent::Loading;
        self.events.trigger();

        if let Some(ref tx) = *self.command_tx.read().unwrap() {
            dlog("Sending load command to player thread");
            if let Err(e) = tx.send(PlayerCommand::Load {
                video_id,
                start_playing,
            }) {
                dlog(&format!("Failed to send Load: {:?}", e));
            }
        } else {
            dlog("Player thread not running — skipping Load");
        }
    }

    pub fn update_track(&self) {
        self.events.trigger();
    }

    pub fn play(&self) {
        dlog("Play");
        if let Some(ref tx) = *self.command_tx.read().unwrap() {
            if let Err(e) = tx.send(PlayerCommand::Play) {
                dlog(&format!("Failed to send Play: {:?}", e));
            }
            *self.status.write().unwrap() = PlayerEvent::Playing(SystemTime::now());
            *self.since.write().unwrap() = Some(SystemTime::now());
            self.events.trigger();
        } else {
            dlog("Player thread not running — skipping Play");
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
            if let Err(e) = tx.send(PlayerCommand::Pause) {
                dlog(&format!("Failed to send Pause: {:?}", e));
            }
            let progress = self.get_current_progress();
            *self.status.write().unwrap() = PlayerEvent::Paused(progress);
            *self.elapsed.write().unwrap() = Some(progress);
            self.events.trigger();
        } else {
            dlog("Player thread not running — skipping Pause");
        }
    }

    pub fn stop(&self) {
        dlog("Stop");
        if let Some(ref tx) = *self.command_tx.read().unwrap() {
            if let Err(e) = tx.send(PlayerCommand::Stop) {
                dlog(&format!("Failed to send Stop: {:?}", e));
            }
            *self.status.write().unwrap() = PlayerEvent::Stopped;
            *self.elapsed.write().unwrap() = None;
            *self.since.write().unwrap() = None;
            self.events.trigger();
        } else {
            dlog("Player thread not running — skipping Stop");
        }
    }

    pub fn seek(&self, position_ms: u32) {
        // For now, only support seeking to position 0 (restart track)
        if position_ms == 0 {
            dlog("Seeking to position 0: sending Restart command");
            if let Some(ref tx) = *self.command_tx.read().unwrap() {
                if let Err(e) = tx.send(PlayerCommand::Restart) {
                    dlog(&format!("Failed to send Restart: {:?}", e));
                }
            } else {
                dlog("Player thread not running — skipping Restart");
            }
        } else {
            dlog(&format!(
                "Seek to position {} not yet implemented",
                position_ms
            ));
        }
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
            if let Err(e) = tx.send(PlayerCommand::SetVolume(volume_f32)) {
                dlog(&format!("Failed to send SetVolume: {:?}", e));
            }
        } else {
            dlog("Player thread not running — skipping SetVolume");
        }
    }

    pub fn set_queue(&self, queue: Arc<RwLock<Vec<Playable>>>) {
        if let Some(ref tx) = *self.command_tx.read().unwrap() {
            if let Err(e) = tx.send(PlayerCommand::SetQueue(queue)) {
                dlog(&format!("Failed to send SetQueue: {:?}", e));
            }
        } else {
            dlog("Player thread not running — skipping SetQueue");
        }
    }

    /// Eagerly resolve 0-duration tracks in the queue via the player API.
    /// Non-blocking — resolution runs asynchronously in the player thread.
    pub fn resolve_durations(&self) {
        if let Some(ref tx) = *self.command_tx.read().unwrap() {
            if let Err(e) = tx.send(PlayerCommand::ResolveDurations) {
                dlog(&format!("Failed to send ResolveDurations: {:?}", e));
            }
        } else {
            dlog("Player thread not running — skipping ResolveDurations");
        }
    }

    /// Pre-download audio for adjacent tracks in the background.
    /// Skips videos that already have a temp file on disk.
    pub fn preload_stream_urls(&self, video_ids: Vec<String>) {
        if let Some(ref tx) = *self.command_tx.read().unwrap() {
            if let Err(e) = tx.send(PlayerCommand::PreloadStreamUrls(video_ids)) {
                dlog(&format!("Failed to send PreloadStreamUrls: {:?}", e));
            }
        } else {
            dlog("Player thread not running — skipping PreloadStreamUrls");
        }
    }

    pub fn shutdown(&self) {
        dlog("Shutting down player");
        if let Some(ref tx) = *self.command_tx.read().unwrap() {
            let _ = tx.send(PlayerCommand::Shutdown);
        }
        *self.command_tx.write().unwrap() = None;
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
    track_finished: Arc<std::sync::atomic::AtomicBool>,
    current_track: Arc<RwLock<Option<Playable>>>,
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

    // UI refresh interval (400ms provides smooth progress bar updates)
    let ui_refresh_interval = Duration::from_millis(400);

    // Shared queue data — set via SetQueue command after Queue construction.
    #[allow(clippy::type_complexity)]
    let queue_data = Arc::<RwLock<Option<Arc<RwLock<Vec<Playable>>>>>>::new(RwLock::new(None));

    // Report a playback failure: reset the (otherwise stuck) Loading state, wake the
    // UI, and surface a short error dialog on the UI thread.
    let report_failure = |reason: String| {
        dlog(&format!("playback failed: {reason}"));
        *status.write().unwrap() = PlayerEvent::FailedToPlay(reason.clone());
        events.trigger();
        events.run_on_ui(move |s| {
            use crate::ui::modal::Modal;
            use cursive::views::Dialog;
            let dialog = Dialog::text(format!(
                "Playback failed: {reason}.\nSee /tmp/ncytm_debug.log for details."
            ))
            .title("Playback error")
            .dismiss_button("Close");
            s.add_layer(Modal::new(dialog));
        });
    };

    loop {
        // Use recv_timeout to allow periodic UI refresh during playback
        match command_rx.recv_timeout(ui_refresh_interval) {
            Ok(cmd) => {
                dlog(&format!("Received command: {:?}", cmd));
                match cmd {
                    PlayerCommand::Load {
                        video_id,
                        start_playing,
                    } => {
                        dlog(&format!("Loading track: {}", video_id));

                        let temp_path = format!("/tmp/ncytm_audio_{}.mp3", video_id);
                        let cache_hit = std::path::Path::new(&temp_path).exists();

                        let stream_result = if cache_hit {
                            dlog(&format!("Cache hit: {}", temp_path));
                            let content_length =
                                std::fs::metadata(&temp_path).ok().map(|m| m.len());
                            Ok(StreamInfo {
                                url: format!("file://{}", temp_path),
                                mime_type: "audio/mpeg".to_string(),
                                codec: "mp3".to_string(),
                                bitrate: 128000,
                                sample_rate: Some(44100),
                                channels: Some(2),
                                content_length,
                                duration_seconds: None,
                                expires_at: None,
                            })
                        } else {
                            dlog(&format!("Cache miss, fetching via yt-dlp: {}", video_id));
                            rt.block_on(async {
                                get_stream_url(&client, &video_id, AudioQuality::High).await
                            })
                        };

                        match stream_result {
                            Ok(stream_info) => {
                                dlog(&format!("Got stream URL, mime: {}", stream_info.mime_type));

                                // If yt-dlp returned a duration and the track has 0:00,
                                // update the track model so the UI shows the correct duration.
                                if let Some(yt_duration) = stream_info.duration_seconds
                                    && let Some(track) = current_track.read().unwrap().as_ref()
                                    && track.duration() == 0
                                    && yt_duration > 0
                                {
                                    dlog(&format!(
                                        "Updating track duration from 0 to {}s (yt-dlp)",
                                        yt_duration
                                    ));
                                    let mut track_mut = current_track.write().unwrap();
                                    if let Some(ref mut t) = *track_mut {
                                        t.set_duration(yt_duration);
                                    }
                                    drop(track_mut);

                                    if let Some(ref q) = *queue_data.read().unwrap() {
                                        let mut q_writer = q.write().unwrap();
                                        for entry in q_writer.iter_mut() {
                                            if entry.duration() == 0
                                                && entry.id() == Some(video_id.clone())
                                            {
                                                entry.set_duration(yt_duration);
                                                dlog(&format!(
                                                    "Updated queue entry duration for {} to {}s",
                                                    video_id, yt_duration
                                                ));
                                            }
                                        }
                                    }
                                }

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
                                        dlog(&format!(
                                            "Failed to load into player (cache_hit={}): {:?}",
                                            cache_hit, e
                                        ));
                                        // Fallback: if a cached file failed to load,
                                        // re-download via yt-dlp
                                        let mut recovered = false;
                                        if cache_hit {
                                            dlog("Falling back to yt-dlp re-download");
                                            match rt.block_on(async {
                                                get_stream_url(
                                                    &client,
                                                    &video_id,
                                                    AudioQuality::High,
                                                )
                                                .await
                                            }) {
                                                Ok(fresh) => {
                                                    match player.load_url(&fresh.url, start_playing)
                                                    {
                                                        Ok(()) => {
                                                            if start_playing {
                                                                *status.write().unwrap() =
                                                                    PlayerEvent::Playing(
                                                                        SystemTime::now(),
                                                                    );
                                                                *since.write().unwrap() =
                                                                    Some(SystemTime::now());
                                                            }
                                                            events.trigger();
                                                            recovered = true;
                                                        }
                                                        Err(e2) => {
                                                            dlog(&format!(
                                                                "Fallback load_url failed: {:?}",
                                                                e2
                                                            ));
                                                        }
                                                    }
                                                }
                                                Err(e2) => {
                                                    dlog(&format!(
                                                        "Fallback get_stream_url failed: {:?}",
                                                        e2
                                                    ));
                                                }
                                            }
                                        }
                                        if !recovered {
                                            report_failure(format!("could not decode audio: {e}"));
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                report_failure(stream_error_message(&e));
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
                    PlayerCommand::SetQueue(q) => {
                        *queue_data.write().unwrap() = Some(q);
                        dlog("Queue data set in player thread");
                    }
                    PlayerCommand::ResolveDurations => {
                        let video_ids: Vec<String> = {
                            let guard = queue_data.read().unwrap();
                            let Some(ref q) = *guard else {
                                dlog("ResolveDurations: no queue data yet");
                                continue;
                            };
                            let reader = q.read().unwrap();
                            reader
                                .iter()
                                .filter(|e| e.duration() == 0)
                                .filter_map(|e| e.id())
                                .map(|s| s.to_string())
                                .collect()
                        };
                        if video_ids.is_empty() {
                            dlog("ResolveDurations: no 0-duration tracks to resolve");
                        } else {
                            dlog(&format!(
                                "ResolveDurations: resolving {} track(s)",
                                video_ids.len()
                            ));
                            let q = queue_data.clone();
                            for video_id in video_ids {
                                let q = q.clone();
                                let client = client.clone();
                                let events = events.clone();
                                rt.spawn(async move {
                                    match get_video_duration(&client, &video_id).await {
                                        Ok(duration) if duration > 0 => {
                                            let guard = q.read().unwrap();
                                            let Some(ref queue_arc) = *guard else {
                                                return;
                                            };
                                            let mut writer = queue_arc.write().unwrap();
                                            for entry in writer.iter_mut() {
                                                if entry.duration() == 0
                                                    && entry.id() == Some(video_id.clone())
                                                {
                                                    dlog(&format!(
                                                        "Resolved duration for {} to {}s",
                                                        video_id, duration
                                                    ));
                                                    entry.set_duration(duration);
                                                    break;
                                                }
                                            }
                                            drop(writer);
                                            events.trigger();
                                        }
                                        _ => {
                                            dlog(&format!(
                                                "ResolveDurations: no duration for {}",
                                                video_id
                                            ));
                                        }
                                    }
                                });
                            }
                        }
                    }
                    PlayerCommand::Restart => {
                        dlog("Restarting current track");
                        if let Some(track) = current_track.read().unwrap().clone() {
                            let video_id = track.id().unwrap_or_default();
                            let was_playing =
                                matches!(*status.read().unwrap(), PlayerEvent::Playing(_));

                            dlog(&format!(
                                "Restarting track {}, was_playing={}",
                                video_id, was_playing
                            ));

                            let temp_path = format!("/tmp/ncytm_audio_{}.mp3", video_id);
                            let cache_hit = std::path::Path::new(&temp_path).exists();

                            let stream_result = if cache_hit {
                                dlog(&format!("Restart cache hit: {}", temp_path));
                                Ok(StreamInfo {
                                    url: format!("file://{}", temp_path),
                                    mime_type: "audio/mpeg".to_string(),
                                    codec: "mp3".to_string(),
                                    bitrate: 128000,
                                    sample_rate: Some(44100),
                                    channels: Some(2),
                                    content_length: std::fs::metadata(&temp_path)
                                        .ok()
                                        .map(|m| m.len()),
                                    duration_seconds: None,
                                    expires_at: None,
                                })
                            } else {
                                dlog("Restart cache miss, fetching via yt-dlp");
                                rt.block_on(async {
                                    get_stream_url(&client, &video_id, AudioQuality::High).await
                                })
                            };

                            match stream_result {
                                Ok(stream_info) => {
                                    if let Some(yt_duration) = stream_info.duration_seconds
                                        && yt_duration > 0
                                    {
                                        dlog(&format!(
                                            "Restart: updating track duration from yt-dlp: {}s",
                                            yt_duration
                                        ));
                                        let mut track_mut = current_track.write().unwrap();
                                        if let Some(ref mut t) = *track_mut
                                            && t.duration() == 0
                                        {
                                            t.set_duration(yt_duration);
                                        }
                                        drop(track_mut);

                                        if let Some(ref q) = *queue_data.read().unwrap() {
                                            let mut q_writer = q.write().unwrap();
                                            for entry in q_writer.iter_mut() {
                                                if entry.duration() == 0
                                                    && entry.id() == Some(video_id.clone())
                                                {
                                                    entry.set_duration(yt_duration);
                                                }
                                            }
                                        }
                                    }

                                    match player.load_url(&stream_info.url, was_playing) {
                                        Ok(()) => {
                                            dlog("Track restarted successfully");
                                            if was_playing {
                                                *status.write().unwrap() =
                                                    PlayerEvent::Playing(SystemTime::now());
                                                *since.write().unwrap() = Some(SystemTime::now());
                                            }
                                            events.trigger();
                                        }
                                        Err(e) => {
                                            dlog(&format!(
                                                "Failed to restart (cache_hit={}): {:?}",
                                                cache_hit, e
                                            ));
                                            if cache_hit {
                                                dlog("Restart fallback: re-downloading via yt-dlp");
                                                if let Ok(fresh) = rt.block_on(async {
                                                    get_stream_url(
                                                        &client,
                                                        &video_id,
                                                        AudioQuality::High,
                                                    )
                                                    .await
                                                }) {
                                                    let _ =
                                                        player.load_url(&fresh.url, was_playing);
                                                    if was_playing {
                                                        *status.write().unwrap() =
                                                            PlayerEvent::Playing(SystemTime::now());
                                                        *since.write().unwrap() =
                                                            Some(SystemTime::now());
                                                    }
                                                    events.trigger();
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    report_failure(stream_error_message(&e));
                                }
                            }
                        } else {
                            dlog("Cannot restart: no current track");
                        }
                    }
                    PlayerCommand::PreloadStreamUrls(video_ids) => {
                        dlog(&format!("Preloading {} stream URL(s)", video_ids.len()));
                        for video_id in video_ids {
                            let temp_path = format!("/tmp/ncytm_audio_{}.mp3", video_id);
                            if std::path::Path::new(&temp_path).exists() {
                                continue;
                            }
                            let client = client.clone();
                            rt.spawn(async move {
                                if let Err(e) =
                                    get_stream_url(&client, &video_id, AudioQuality::High).await
                                {
                                    dlog(&format!("Preload failed for {}: {:?}", video_id, e));
                                }
                            });
                        }
                    }
                    PlayerCommand::Shutdown => {
                        dlog("Shutting down player thread");
                        player.stop();
                        break;
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Periodically trigger UI refresh while playing
                // This updates the progress bar and elapsed time display
                let current_status = status.read().unwrap().clone();
                if matches!(current_status, PlayerEvent::Playing(_)) {
                    // Check if track has finished playing
                    if player.is_finished() {
                        dlog("Track finished - setting flag for main thread");
                        *status.write().unwrap() = PlayerEvent::Stopped;
                        track_finished.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    events.trigger();
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                dlog("Command channel closed");
                break;
            }
        }
    }
}
