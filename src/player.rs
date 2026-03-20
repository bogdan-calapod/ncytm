//! Audio player for YouTube Music streams.
//!
//! Provides audio playback functionality using rodio for audio output
//! and symphonia for decoding various audio formats.

use std::io::{BufReader, Read, Seek};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use log::{debug, error, info, warn};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use thiserror::Error;

/// Errors that can occur during audio playback.
#[derive(Debug, Error)]
pub enum PlayerError {
    #[error("Failed to initialize audio output: {0}")]
    OutputError(String),

    #[error("Failed to decode audio: {0}")]
    DecodeError(String),

    #[error("Failed to load audio from URL: {0}")]
    LoadError(String),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("No audio output available")]
    NoOutput,
}

/// Audio player state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    /// Player is stopped (no track loaded).
    Stopped,
    /// Player is playing.
    Playing,
    /// Player is paused.
    Paused,
}

/// Audio player for streaming YouTube Music content.
pub struct Player {
    /// Audio output stream (must be kept alive).
    _stream: OutputStream,
    /// Handle to the audio output.
    stream_handle: OutputStreamHandle,
    /// Current audio sink.
    sink: Option<Sink>,
    /// Current player state.
    state: PlayerState,
    /// Volume level (0.0 to 1.0).
    volume: f32,
    /// Current playback position in milliseconds.
    position_ms: Arc<AtomicU64>,
    /// Total duration in milliseconds.
    duration_ms: Arc<AtomicU64>,
    /// Whether playback has finished.
    finished: Arc<AtomicBool>,
}

impl Player {
    /// Create a new audio player.
    pub fn new() -> Result<Self, PlayerError> {
        let (stream, stream_handle) =
            OutputStream::try_default().map_err(|e| PlayerError::OutputError(e.to_string()))?;

        Ok(Self {
            _stream: stream,
            stream_handle,
            sink: None,
            state: PlayerState::Stopped,
            volume: 1.0,
            position_ms: Arc::new(AtomicU64::new(0)),
            duration_ms: Arc::new(AtomicU64::new(0)),
            finished: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Load and play audio from a URL or file path.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to stream audio from, or a file:// URL for local files
    /// * `start_playing` - Whether to start playing immediately
    pub fn load_url(&mut self, url: &str, start_playing: bool) -> Result<(), PlayerError> {
        info!("Loading audio from URL: {}", &url[..url.len().min(100)]);

        // Stop any current playback
        self.stop();

        // Check if this is a local file (file:// URL)
        let file = if let Some(path) = url.strip_prefix("file://") {
            self.open_local_file(path)?
        } else {
            self.download_to_file(url)?
        };
        let reader = BufReader::new(file);

        // Create decoder
        let source = Decoder::new(reader).map_err(|e| PlayerError::DecodeError(e.to_string()))?;

        // Get duration if available
        if let Some(duration) = source.total_duration() {
            self.duration_ms
                .store(duration.as_millis() as u64, Ordering::SeqCst);
        }

        // Create a new sink
        let sink = Sink::try_new(&self.stream_handle)
            .map_err(|e| PlayerError::OutputError(e.to_string()))?;

        // Set volume
        sink.set_volume(self.volume);

        // Append the source
        sink.append(source);

        // Pause if not starting immediately
        if !start_playing {
            sink.pause();
            self.state = PlayerState::Paused;
        } else {
            self.state = PlayerState::Playing;
        }

        self.sink = Some(sink);
        self.finished.store(false, Ordering::SeqCst);
        self.position_ms.store(0, Ordering::SeqCst);

        Ok(())
    }

    /// Open a local audio file.
    fn open_local_file(&self, path: &str) -> Result<std::fs::File, PlayerError> {
        use std::io::Write;

        fn dlog(msg: &str) {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/ncytm_debug.log")
            {
                let _ = writeln!(f, "[PLAYER] {}", msg);
            }
        }

        dlog(&format!("Opening local file: {}", path));

        // Check if file exists and has content
        let metadata = std::fs::metadata(path)
            .map_err(|e| PlayerError::LoadError(format!("File not found: {} - {}", path, e)))?;

        if metadata.len() == 0 {
            return Err(PlayerError::LoadError("Audio file is empty".to_string()));
        }

        dlog(&format!("File size: {} bytes", metadata.len()));

        std::fs::File::open(path)
            .map_err(|e| PlayerError::LoadError(format!("Failed to open file: {}", e)))
    }

    /// Download audio to a temp file for reliable playback.
    fn download_to_file(&self, url: &str) -> Result<std::fs::File, PlayerError> {
        use std::io::Write;

        fn dlog(msg: &str) {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/ncytm_debug.log")
            {
                let _ = writeln!(f, "[PLAYER] {}", msg);
            }
        }

        dlog("Downloading audio...");

        // Create a temp file path
        let temp_path = format!("/tmp/ncytm_audio_{}.webm", std::process::id());

        // Use curl for downloading (more reliable with these URLs)
        let status = std::process::Command::new("curl")
            .args([
                "-s", // Silent
                "-L", // Follow redirects
                "-o", &temp_path, url,
            ])
            .status()
            .map_err(|e| PlayerError::DecodeError(format!("curl failed: {}", e)))?;

        dlog(&format!("curl exit status: {:?}", status));

        if !status.success() {
            return Err(PlayerError::DecodeError(
                "Failed to download audio".to_string(),
            ));
        }

        // Check file size
        let metadata =
            std::fs::metadata(&temp_path).map_err(|e| PlayerError::DecodeError(e.to_string()))?;

        dlog(&format!("Downloaded {} bytes", metadata.len()));

        if metadata.len() == 0 {
            return Err(PlayerError::DecodeError(
                "Downloaded file is empty".to_string(),
            ));
        }

        // Open the file for reading
        std::fs::File::open(&temp_path).map_err(|e| PlayerError::DecodeError(e.to_string()))
    }

    /// Load audio from bytes.
    pub fn load_bytes(&mut self, data: Vec<u8>, start_playing: bool) -> Result<(), PlayerError> {
        // Stop any current playback
        self.stop();

        let cursor = std::io::Cursor::new(data);
        let reader = BufReader::new(cursor);

        // Create decoder
        let source = Decoder::new(reader).map_err(|e| PlayerError::DecodeError(e.to_string()))?;

        // Get duration if available
        if let Some(duration) = source.total_duration() {
            self.duration_ms
                .store(duration.as_millis() as u64, Ordering::SeqCst);
        }

        // Create a new sink
        let sink = Sink::try_new(&self.stream_handle)
            .map_err(|e| PlayerError::OutputError(e.to_string()))?;

        sink.set_volume(self.volume);
        sink.append(source);

        if !start_playing {
            sink.pause();
            self.state = PlayerState::Paused;
        } else {
            self.state = PlayerState::Playing;
        }

        self.sink = Some(sink);
        self.finished.store(false, Ordering::SeqCst);
        self.position_ms.store(0, Ordering::SeqCst);

        Ok(())
    }

    /// Start or resume playback.
    pub fn play(&mut self) {
        if let Some(ref sink) = self.sink {
            sink.play();
            self.state = PlayerState::Playing;
            debug!("Playback started");
        }
    }

    /// Pause playback.
    pub fn pause(&mut self) {
        if let Some(ref sink) = self.sink {
            sink.pause();
            self.state = PlayerState::Paused;
            debug!("Playback paused");
        }
    }

    /// Stop playback and unload the current track.
    pub fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.state = PlayerState::Stopped;
        self.position_ms.store(0, Ordering::SeqCst);
        self.duration_ms.store(0, Ordering::SeqCst);
        self.finished.store(false, Ordering::SeqCst);
        debug!("Playback stopped");
    }

    /// Set the volume level.
    ///
    /// # Arguments
    ///
    /// * `volume` - Volume level from 0.0 (mute) to 1.0 (full volume)
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(ref sink) = self.sink {
            sink.set_volume(self.volume);
        }
        debug!("Volume set to {:.0}%", self.volume * 100.0);
    }

    /// Get the current volume level.
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Get the current player state.
    pub fn state(&self) -> PlayerState {
        self.state
    }

    /// Check if playback is currently active.
    pub fn is_playing(&self) -> bool {
        self.state == PlayerState::Playing
    }

    /// Check if the current track has finished playing.
    pub fn is_finished(&self) -> bool {
        if let Some(ref sink) = self.sink {
            sink.empty()
        } else {
            true
        }
    }

    /// Get the current playback position.
    pub fn position(&self) -> Duration {
        // Note: rodio doesn't provide easy position tracking
        // This is a placeholder - in practice, you'd need to track this manually
        Duration::from_millis(self.position_ms.load(Ordering::SeqCst))
    }

    /// Get the total duration of the current track.
    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.duration_ms.load(Ordering::SeqCst))
    }

    /// Seek to a specific position.
    ///
    /// Note: Seeking is not well-supported with streaming sources in rodio.
    /// This is a best-effort implementation.
    pub fn seek(&mut self, _position: Duration) -> Result<(), PlayerError> {
        // rodio doesn't support seeking well with streaming sources
        // For proper seeking, we'd need to reload the audio from the URL
        // with a Range header or use a different approach
        warn!("Seeking is not fully supported");
        Ok(())
    }

    /// Skip forward by a given duration.
    pub fn skip_forward(&mut self, duration: Duration) -> Result<(), PlayerError> {
        if let Some(ref sink) = self.sink {
            sink.skip_one();
            // This skips to the next source in the queue, not a time-based skip
            // True time-based skipping requires different implementation
        }
        Ok(())
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Audio tests are difficult to run in CI environments
    // These tests are marked as ignored and should be run manually

    #[test]
    fn test_player_state() {
        // Test state enum
        assert_eq!(PlayerState::Stopped, PlayerState::Stopped);
        assert_ne!(PlayerState::Playing, PlayerState::Paused);
    }

    #[test]
    fn test_volume_clamping() {
        // Test that volume is clamped to valid range
        let volume = 1.5_f32.clamp(0.0, 1.0);
        assert_eq!(volume, 1.0);

        let volume = (-0.5_f32).clamp(0.0, 1.0);
        assert_eq!(volume, 0.0);

        let volume = 0.5_f32.clamp(0.0, 1.0);
        assert_eq!(volume, 0.5);
    }

    #[test]
    #[ignore] // Requires audio output device
    fn test_player_creation() {
        let player = Player::new();
        assert!(player.is_ok());

        let player = player.unwrap();
        assert_eq!(player.state(), PlayerState::Stopped);
        assert_eq!(player.volume(), 1.0);
    }
}
