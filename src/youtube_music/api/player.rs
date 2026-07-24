//! YouTube Music player API.
//!
//! Fetches video metadata (duration, title, etc.) from the player endpoint.
//! Used as a fallback when list responses don't include duration info.

use serde_json::json;

use crate::youtube_music::{ClientError, YouTubeMusicClient};

/// Fetch a video's duration in seconds from the player endpoint.
///
/// This is a lightweight metadata lookup — it only extracts `videoDetails.lengthSeconds`
/// without parsing streaming formats.
pub async fn get_video_duration(
    client: &YouTubeMusicClient,
    video_id: &str,
) -> Result<u32, ClientError> {
    let body = json!({
        "videoId": video_id,
        "playbackContext": {
            "contentPlaybackContext": {
                "signatureTimestamp": 19950
            }
        }
    });

    let response = client.post("player", &body).await?;

    let duration_str = response
        .pointer("/videoDetails/lengthSeconds")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ClientError::ApiError {
            message: "No lengthSeconds in player response".to_string(),
        })?;

    duration_str
        .parse::<u32>()
        .map_err(|_| ClientError::ApiError {
            message: format!("Invalid lengthSeconds: {}", duration_str),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duration_extraction() {
        let response = serde_json::json!({
            "videoDetails": {
                "lengthSeconds": "213"
            }
        });
        let duration = response
            .pointer("/videoDetails/lengthSeconds")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u32>().ok());
        assert_eq!(duration, Some(213));
    }

    #[test]
    fn test_missing_video_details() {
        let response = serde_json::json!({});
        let duration = response
            .pointer("/videoDetails/lengthSeconds")
            .and_then(|v| v.as_str());
        assert!(duration.is_none());
    }
}
