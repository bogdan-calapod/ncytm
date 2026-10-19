//! YouTube Music player API.
//!
//! Fetches video metadata (duration, title, etc.) from the player endpoint.
//! Used as a fallback when list responses don't include duration info, and
//! as the primary source of metadata when resolving a video ID directly
//! (search-by-video-ID is unreliable — YouTube never returns the exact video
//! for IDs that start with `-`, which its search parser treats as an
//! exclusion operator).

use serde_json::json;

use crate::youtube_music::{ClientError, YouTubeMusicClient};

/// Metadata for a single video, fetched from the player endpoint.
#[derive(Debug, Clone)]
pub struct VideoMetadata {
    /// YouTube video ID.
    pub video_id: String,
    /// Video title.
    pub title: String,
    /// Author (artist) name. Already cleaned up by YouTube — auto-generated
    /// Topic channels yield the plain artist name, not "Artist - Topic".
    pub author: String,
    /// Author's channel browse ID (starts with "UC"), if available.
    pub channel_id: Option<String>,
    /// Duration in seconds.
    pub length_seconds: u32,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
}

/// Build the request body for a player endpoint lookup.
fn player_request_body(video_id: &str) -> serde_json::Value {
    json!({
        "videoId": video_id,
        "playbackContext": {
            "contentPlaybackContext": {
                "signatureTimestamp": 19950
            }
        }
    })
}

/// Fetch a video's duration in seconds from the player endpoint.
///
/// This is a lightweight metadata lookup — it only extracts `videoDetails.lengthSeconds`
/// without parsing streaming formats.
pub async fn get_video_duration(
    client: &YouTubeMusicClient,
    video_id: &str,
) -> Result<u32, ClientError> {
    let response = client
        .post("player", &player_request_body(video_id))
        .await?;

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

/// Fetch a video's metadata (title, author, channel, duration, thumbnail)
/// from the player endpoint.
///
/// Unlike search-based resolution, this works for any video ID — including
/// IDs starting with `-`, which YouTube's search parser treats as an
/// exclusion operator.
pub async fn get_video_metadata(
    client: &YouTubeMusicClient,
    video_id: &str,
) -> Result<VideoMetadata, ClientError> {
    let response = client
        .post("player", &player_request_body(video_id))
        .await?;
    parse_video_metadata(&response, video_id)
}

/// Parse a player endpoint response into [`VideoMetadata`].
fn parse_video_metadata(
    response: &serde_json::Value,
    video_id: &str,
) -> Result<VideoMetadata, ClientError> {
    let details = response
        .pointer("/videoDetails")
        .ok_or_else(|| ClientError::ApiError {
            message: "No videoDetails in player response".to_string(),
        })?;

    let title = details
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ClientError::ApiError {
            message: "No title in player response".to_string(),
        })?
        .to_string();

    let author = details
        .get("author")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let channel_id = details
        .get("channelId")
        .and_then(|v| v.as_str())
        .map(String::from);

    let length_seconds = details
        .get("lengthSeconds")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let thumbnail_url = details
        .pointer("/thumbnail/thumbnails")
        .and_then(|v| v.as_array())
        .and_then(|thumbs| thumbs.last())
        .and_then(|t| t.get("url"))
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(VideoMetadata {
        video_id: video_id.to_string(),
        title,
        author,
        channel_id,
        length_seconds,
        thumbnail_url,
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

    #[test]
    fn test_parse_video_metadata() {
        // Trimmed real response for a Topic-channel upload whose video ID
        // starts with a hyphen ("-YwtksWAEKs").
        let response = serde_json::json!({
            "playabilityStatus": {"status": "UNPLAYABLE"},
            "videoDetails": {
                "videoId": "-YwtksWAEKs",
                "title": "Poor Unfortunate Souls",
                "author": "Jonathan Young",
                "channelId": "UCR-DFuX6gcT4KXJqmOQAULA",
                "lengthSeconds": "175",
                "thumbnail": {
                    "thumbnails": [
                        {"url": "https://i.ytimg.com/vi/-YwtksWAEKs/small.jpg", "width": 120, "height": 120},
                        {"url": "https://i.ytimg.com/vi/-YwtksWAEKs/large.jpg", "width": 544, "height": 544}
                    ]
                }
            }
        });

        let metadata = parse_video_metadata(&response, "-YwtksWAEKs").unwrap();
        assert_eq!(metadata.video_id, "-YwtksWAEKs");
        assert_eq!(metadata.title, "Poor Unfortunate Souls");
        assert_eq!(metadata.author, "Jonathan Young");
        assert_eq!(
            metadata.channel_id.as_deref(),
            Some("UCR-DFuX6gcT4KXJqmOQAULA")
        );
        assert_eq!(metadata.length_seconds, 175);
        // Uses the largest thumbnail available.
        assert_eq!(
            metadata.thumbnail_url.as_deref(),
            Some("https://i.ytimg.com/vi/-YwtksWAEKs/large.jpg")
        );
    }

    #[test]
    fn test_parse_video_metadata_missing_fields() {
        let response = serde_json::json!({
            "videoDetails": {
                "title": "Untitled"
            }
        });

        let metadata = parse_video_metadata(&response, "abc123").unwrap();
        assert_eq!(metadata.video_id, "abc123");
        assert_eq!(metadata.title, "Untitled");
        assert_eq!(metadata.author, "");
        assert_eq!(metadata.channel_id, None);
        assert_eq!(metadata.length_seconds, 0);
        assert_eq!(metadata.thumbnail_url, None);
    }

    #[test]
    fn test_parse_video_metadata_without_details() {
        let response = serde_json::json!({"playabilityStatus": {"status": "ERROR"}});
        assert!(parse_video_metadata(&response, "abc123").is_err());
    }

    #[test]
    fn test_parse_video_metadata_without_title() {
        let response = serde_json::json!({"videoDetails": {"author": "Someone"}});
        assert!(parse_video_metadata(&response, "abc123").is_err());
    }

    // Integration test for get_video_metadata - requires real cookies to run
    // Run with: cargo test --ignored -- test_get_video_metadata_integration
    #[tokio::test]
    #[ignore]
    async fn test_get_video_metadata_integration() {
        // This test requires actual cookies from ~/.config/ncytm/cookies.txt
        use crate::youtube_music::Cookies;
        use std::path::PathBuf;

        let cookies_path =
            PathBuf::from(std::env::var("HOME").unwrap()).join(".config/ncytm/cookies.txt");

        if !cookies_path.exists() {
            eprintln!(
                "Skipping integration test - no cookies file found at {:?}",
                cookies_path
            );
            return;
        }

        let cookies = Cookies::from_file(&cookies_path).expect("Failed to load cookies");
        let client = YouTubeMusicClient::new(cookies).expect("Failed to create client");

        // Video ID starting with a hyphen, which search-based lookup cannot
        // resolve (YouTube treats it as an exclusion operator).
        let metadata = get_video_metadata(&client, "-YwtksWAEKs")
            .await
            .expect("Failed to fetch metadata");

        assert_eq!(metadata.video_id, "-YwtksWAEKs");
        assert!(!metadata.title.is_empty());
        assert!(!metadata.author.is_empty());
        assert!(metadata.length_seconds > 0);
    }
}
