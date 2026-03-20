//! YouTube Music stream URL extraction.
//!
//! Extracts playable audio stream URLs from YouTube video IDs.

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use super::client::YouTubeMusicClient;

/// Errors that can occur during stream extraction.
#[derive(Debug, Error)]
pub enum StreamError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("API error: {message}")]
    ApiError { message: String },

    #[error("Video not found: {video_id}")]
    VideoNotFound { video_id: String },

    #[error("Video is not playable: {reason}")]
    NotPlayable { reason: String },

    #[error("No audio streams available")]
    NoAudioStreams,

    #[error("Failed to parse stream data: {0}")]
    ParseError(String),
}

/// Information about an audio stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    /// The URL to stream audio from.
    pub url: String,
    /// MIME type (e.g., "audio/webm", "audio/mp4").
    pub mime_type: String,
    /// Audio codec (e.g., "opus", "mp4a.40.2").
    pub codec: String,
    /// Bitrate in bits per second.
    pub bitrate: u32,
    /// Sample rate in Hz.
    pub sample_rate: Option<u32>,
    /// Number of audio channels.
    pub channels: Option<u32>,
    /// Content length in bytes.
    pub content_length: Option<u64>,
    /// Duration in seconds.
    pub duration_seconds: Option<u32>,
    /// When this URL expires (if known).
    pub expires_at: Option<SystemTime>,
}

/// Audio quality preference for stream selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioQuality {
    /// Highest quality available.
    #[default]
    High,
}

/// Get the best audio stream URL for a video.
///
/// # Arguments
///
/// * `client` - The YouTube Music API client
/// * `video_id` - The YouTube video ID
/// * `quality` - Preferred audio quality
///
/// # Returns
///
/// Stream information including the playable URL.
pub async fn get_stream_url(
    client: &YouTubeMusicClient,
    video_id: &str,
    quality: AudioQuality,
) -> Result<StreamInfo, StreamError> {
    let streams = get_audio_streams(client, video_id).await?;

    if streams.is_empty() {
        return Err(StreamError::NoAudioStreams);
    }

    // Select stream based on quality preference
    let stream = select_best_stream(&streams, quality);

    Ok(stream.clone())
}

fn dlog(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/ncytm_debug.log")
    {
        let _ = writeln!(f, "[STREAM] {}", msg);
    }
}

/// Get audio stream for a video.
///
/// Uses yt-dlp to download directly to a temp file and returns the file path.
pub async fn get_audio_streams(
    _client: &YouTubeMusicClient,
    video_id: &str,
) -> Result<Vec<StreamInfo>, StreamError> {
    dlog(&format!("Downloading audio for {} with yt-dlp", video_id));

    // Download directly to a temp file
    // Use mp3 format for best compatibility with rodio/symphonia
    let temp_path = format!("/tmp/ncytm_audio_{}.mp3", video_id);

    // Remove old file if exists
    let _ = std::fs::remove_file(&temp_path);

    let output = std::process::Command::new("yt-dlp")
        .args([
            "-f",
            "bestaudio",
            "-x", // Extract audio
            "--audio-format",
            "mp3",
            "-o",
            &temp_path,
            "--no-warnings",
            "--no-progress",
            &format!("https://music.youtube.com/watch?v={}", video_id),
        ])
        .output();

    match output {
        Ok(out) => {
            if out.status.success() {
                // Check if file exists and has content
                if let Ok(metadata) = std::fs::metadata(&temp_path)
                    && metadata.len() > 0
                {
                    dlog(&format!(
                        "yt-dlp downloaded {} bytes to {}",
                        metadata.len(),
                        temp_path
                    ));
                    return Ok(vec![StreamInfo {
                        url: format!("file://{}", temp_path), // Use file:// URL
                        mime_type: "audio/mpeg".to_string(),
                        codec: "mp3".to_string(),
                        bitrate: 128000,
                        sample_rate: Some(44100),
                        channels: Some(2),
                        content_length: Some(metadata.len()),
                        duration_seconds: None,
                        expires_at: None,
                    }]);
                }
            }
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            dlog(&format!(
                "yt-dlp failed - stderr: {}, stdout: {}",
                stderr, stdout
            ));
            Err(StreamError::ApiError {
                message: format!("yt-dlp failed: {}", stderr),
            })
        }
        Err(e) => {
            dlog(&format!("yt-dlp error: {}", e));
            Err(StreamError::ApiError {
                message: format!("yt-dlp not available: {}", e),
            })
        }
    }
}

/// Get audio streams using innertube API (fallback, may not work due to cipher).
#[allow(dead_code)]
async fn get_audio_streams_innertube(
    client: &YouTubeMusicClient,
    video_id: &str,
) -> Result<Vec<StreamInfo>, StreamError> {
    let body = json!({
        "videoId": video_id,
        "playbackContext": {
            "contentPlaybackContext": {
                "signatureTimestamp": get_signature_timestamp()
            }
        }
    });

    let response = client
        .post("player", &body)
        .await
        .map_err(|e| StreamError::ApiError {
            message: e.to_string(),
        })?;

    // Check for playability status
    check_playability(&response, video_id)?;

    // Extract streaming data
    let streaming_data = response
        .get("streamingData")
        .ok_or_else(|| StreamError::ParseError("No streaming data in response".to_string()))?;

    // Parse adaptive formats (audio-only streams)
    let adaptive_formats = streaming_data
        .get("adaptiveFormats")
        .and_then(|f| f.as_array())
        .ok_or_else(|| StreamError::NoAudioStreams)?;

    let mut streams = Vec::new();

    for format in adaptive_formats {
        // Only include audio formats
        let mime_type = format
            .get("mimeType")
            .and_then(|m| m.as_str())
            .unwrap_or("");

        if !mime_type.starts_with("audio/") {
            continue;
        }

        if let Some(stream) = parse_stream_format(format) {
            streams.push(stream);
        }
    }

    // Sort by bitrate (highest first)
    streams.sort_by(|a, b| b.bitrate.cmp(&a.bitrate));

    Ok(streams)
}

/// Check if the video is playable.
fn check_playability(response: &Value, video_id: &str) -> Result<(), StreamError> {
    let playability = response.get("playabilityStatus");

    let status = playability
        .and_then(|p| p.get("status"))
        .and_then(|s| s.as_str())
        .unwrap_or("UNKNOWN");

    match status {
        "OK" => Ok(()),
        "UNPLAYABLE" | "LOGIN_REQUIRED" | "ERROR" => {
            let reason = playability
                .and_then(|p| p.get("reason"))
                .and_then(|r| r.as_str())
                .unwrap_or("Unknown reason")
                .to_string();
            Err(StreamError::NotPlayable { reason })
        }
        "CONTENT_CHECK_REQUIRED" => Err(StreamError::NotPlayable {
            reason: "Age verification required".to_string(),
        }),
        _ => {
            // Check if video exists
            if response.get("videoDetails").is_none() {
                Err(StreamError::VideoNotFound {
                    video_id: video_id.to_string(),
                })
            } else {
                Ok(())
            }
        }
    }
}

/// Parse a stream format from the API response.
fn parse_stream_format(format: &Value) -> Option<StreamInfo> {
    // Get URL - might be in 'url' or need to be constructed from 'signatureCipher'
    let url = format
        .get("url")
        .and_then(|u| u.as_str())
        .map(String::from)?;

    let mime_type_full = format.get("mimeType").and_then(|m| m.as_str())?;

    // Parse mime type and codec (e.g., "audio/webm; codecs=\"opus\"")
    let (mime_type, codec) = parse_mime_type(mime_type_full);

    let bitrate = format.get("bitrate").and_then(|b| b.as_u64()).unwrap_or(0) as u32;

    let sample_rate = format
        .get("audioSampleRate")
        .and_then(|s| s.as_str())
        .and_then(|s| s.parse().ok());

    let channels = format
        .get("audioChannels")
        .and_then(|c| c.as_u64())
        .map(|c| c as u32);

    let content_length = format
        .get("contentLength")
        .and_then(|c| c.as_str())
        .and_then(|c| c.parse().ok());

    let duration_seconds = format
        .get("approxDurationMs")
        .and_then(|d| d.as_str())
        .and_then(|d| d.parse::<u64>().ok())
        .map(|ms| (ms / 1000) as u32);

    // Calculate expiration time from URL parameter
    let expires_at = extract_expiration(&url);

    Some(StreamInfo {
        url,
        mime_type: mime_type.to_string(),
        codec: codec.to_string(),
        bitrate,
        sample_rate,
        channels,
        content_length,
        duration_seconds,
        expires_at,
    })
}

/// Parse mime type string to extract type and codec.
fn parse_mime_type(mime_type: &str) -> (&str, &str) {
    // Format: "audio/webm; codecs=\"opus\""
    let parts: Vec<&str> = mime_type.split(';').collect();
    let base_type = parts[0].trim();

    let codec = parts
        .get(1)
        .and_then(|c| {
            c.trim()
                .strip_prefix("codecs=\"")
                .and_then(|c| c.strip_suffix('"'))
        })
        .unwrap_or("unknown");

    (base_type, codec)
}

/// Extract expiration time from stream URL.
fn extract_expiration(url: &str) -> Option<SystemTime> {
    // URL contains 'expire=<timestamp>' parameter
    // Split on both '?' and '&' to handle query string properly
    url.split(['?', '&'])
        .find(|p| p.starts_with("expire="))
        .and_then(|p| p.strip_prefix("expire="))
        .and_then(|ts| ts.parse::<u64>().ok())
        .map(|ts| SystemTime::UNIX_EPOCH + Duration::from_secs(ts))
}

/// Select the best stream based on quality preference.
fn select_best_stream(streams: &[StreamInfo], _quality: AudioQuality) -> &StreamInfo {
    // Streams are already sorted by bitrate (highest first)
    // Currently only High quality is supported
    &streams[0]
}

/// Get a signature timestamp for the player request.
/// This is typically extracted from the YouTube page, but we use a default value.
fn get_signature_timestamp() -> u64 {
    // This value changes periodically with YouTube updates
    // A hardcoded value works for most cases
    // In production, this should be extracted from YouTube's base.js
    19950
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mime_type() {
        let (mime, codec) = parse_mime_type("audio/webm; codecs=\"opus\"");
        assert_eq!(mime, "audio/webm");
        assert_eq!(codec, "opus");

        let (mime, codec) = parse_mime_type("audio/mp4; codecs=\"mp4a.40.2\"");
        assert_eq!(mime, "audio/mp4");
        assert_eq!(codec, "mp4a.40.2");

        let (mime, codec) = parse_mime_type("audio/webm");
        assert_eq!(mime, "audio/webm");
        assert_eq!(codec, "unknown");
    }

    #[test]
    fn test_extract_expiration() {
        let url = "https://example.com/stream?expire=1735689600&sig=abc123";
        let expiration = extract_expiration(url);
        assert!(expiration.is_some());

        let url_no_expire = "https://example.com/stream?sig=abc123";
        let expiration = extract_expiration(url_no_expire);
        assert!(expiration.is_none());
    }

    #[test]
    fn test_select_best_stream() {
        let streams = vec![
            StreamInfo {
                url: "high".to_string(),
                mime_type: "audio/webm".to_string(),
                codec: "opus".to_string(),
                bitrate: 256000,
                sample_rate: Some(48000),
                channels: Some(2),
                content_length: None,
                duration_seconds: Some(180),
                expires_at: None,
            },
            StreamInfo {
                url: "medium".to_string(),
                mime_type: "audio/webm".to_string(),
                codec: "opus".to_string(),
                bitrate: 128000,
                sample_rate: Some(48000),
                channels: Some(2),
                content_length: None,
                duration_seconds: Some(180),
                expires_at: None,
            },
            StreamInfo {
                url: "low".to_string(),
                mime_type: "audio/webm".to_string(),
                codec: "opus".to_string(),
                bitrate: 64000,
                sample_rate: Some(48000),
                channels: Some(2),
                content_length: None,
                duration_seconds: Some(180),
                expires_at: None,
            },
        ];

        // Currently only High quality is supported, so all streams return the highest quality
        assert_eq!(select_best_stream(&streams, AudioQuality::High).url, "high");
    }

    #[test]
    fn test_stream_info_serialization() {
        let stream = StreamInfo {
            url: "https://example.com/stream".to_string(),
            mime_type: "audio/webm".to_string(),
            codec: "opus".to_string(),
            bitrate: 128000,
            sample_rate: Some(48000),
            channels: Some(2),
            content_length: Some(1000000),
            duration_seconds: Some(180),
            expires_at: None,
        };

        let json = serde_json::to_string(&stream).unwrap();
        let deserialized: StreamInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.url, stream.url);
        assert_eq!(deserialized.bitrate, stream.bitrate);
    }

    #[test]
    fn test_check_playability_ok() {
        let response = serde_json::json!({
            "playabilityStatus": {
                "status": "OK"
            },
            "videoDetails": {}
        });
        assert!(check_playability(&response, "test123").is_ok());
    }

    #[test]
    fn test_check_playability_unplayable() {
        let response = serde_json::json!({
            "playabilityStatus": {
                "status": "UNPLAYABLE",
                "reason": "Video is private"
            }
        });
        let result = check_playability(&response, "test123");
        assert!(matches!(result, Err(StreamError::NotPlayable { .. })));
    }

    #[test]
    fn test_check_playability_not_found() {
        let response = serde_json::json!({
            "playabilityStatus": {
                "status": "UNKNOWN"
            }
        });
        let result = check_playability(&response, "test123");
        assert!(matches!(result, Err(StreamError::VideoNotFound { .. })));
    }
}
