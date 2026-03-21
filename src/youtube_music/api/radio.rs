//! YouTube Music radio/similar tracks API.
//!
//! Provides functionality to get radio (similar tracks) based on a seed track or playlist.

use serde_json::{Value, json};

use super::library::{AlbumRef, ArtistRef};
use crate::youtube_music::{ClientError, YouTubeMusicClient};

/// A track from the radio/recommendations.
#[derive(Debug, Clone)]
pub struct RadioTrack {
    /// YouTube video ID.
    pub video_id: String,
    /// Track title.
    pub title: String,
    /// Artist names.
    pub artists: Vec<ArtistRef>,
    /// Album reference (if available).
    pub album: Option<AlbumRef>,
    /// Duration in seconds.
    pub duration_seconds: Option<u32>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Whether this is an explicit track.
    pub is_explicit: bool,
}

/// Response containing radio tracks.
#[derive(Debug, Clone)]
pub struct RadioResponse {
    /// The tracks in the radio.
    pub tracks: Vec<RadioTrack>,
    /// Playlist ID for the radio (can be used to get more tracks).
    #[allow(dead_code)]
    pub playlist_id: Option<String>,
}

/// Get radio (similar tracks) based on a video ID.
///
/// This uses the YouTube Music "next" endpoint to get a mix/radio based on the seed track.
///
/// # Arguments
///
/// * `client` - The YouTube Music API client
/// * `video_id` - The video ID to base the radio on
///
/// # Returns
///
/// A response containing similar tracks.
pub async fn get_radio(
    client: &YouTubeMusicClient,
    video_id: &str,
) -> Result<RadioResponse, ClientError> {
    // The "next" endpoint with enablePersistentPlaylistPanel returns radio tracks
    // We need to use the watchEndpoint with the video ID and request automix/radio
    let body = json!({
        "videoId": video_id,
        "isAudioOnly": true,
        "tunerSettingValue": "AUTOMIX_SETTING_NORMAL",
        "watchEndpointMusicSupportedConfigs": {
            "watchEndpointMusicConfig": {
                "hasPersistentPlaylistPanel": true,
                "musicVideoType": "MUSIC_VIDEO_TYPE_ATV"
            }
        },
        "playlistId": format!("RDAMVM{}", video_id)
    });

    let response = client.post("next", &body).await?;
    parse_radio_response(&response, video_id)
}

/// Get radio based on a playlist ID.
///
/// # Arguments
///
/// * `client` - The YouTube Music API client
/// * `playlist_id` - The playlist ID to base the radio on
///
/// # Returns
///
/// A response containing similar tracks.
#[allow(dead_code)]
pub async fn get_playlist_radio(
    client: &YouTubeMusicClient,
    playlist_id: &str,
) -> Result<RadioResponse, ClientError> {
    // For playlist radio, use RDAMPL prefix
    let radio_playlist_id = if playlist_id.starts_with("RDAMPL") {
        playlist_id.to_string()
    } else {
        format!("RDAMPL{}", playlist_id)
    };

    let body = json!({
        "playlistId": radio_playlist_id,
        "isAudioOnly": true,
        "tunerSettingValue": "AUTOMIX_SETTING_NORMAL",
        "watchEndpointMusicSupportedConfigs": {
            "watchEndpointMusicConfig": {
                "hasPersistentPlaylistPanel": true,
                "musicVideoType": "MUSIC_VIDEO_TYPE_ATV"
            }
        }
    });

    let response = client.post("next", &body).await?;
    parse_radio_response(&response, "")
}

/// Parse the radio response from the "next" endpoint.
fn parse_radio_response(
    response: &Value,
    seed_video_id: &str,
) -> Result<RadioResponse, ClientError> {
    let mut tracks = Vec::new();
    let mut playlist_id = None;

    // Try to find the playlist panel renderer which contains the radio tracks
    // Path: contents.singleColumnMusicWatchNextResultsRenderer.tabbedRenderer.watchNextTabbedResultsRenderer
    //       .tabs[0].tabRenderer.content.musicQueueRenderer.content.playlistPanelRenderer

    let panel = response
        .pointer("/contents/singleColumnMusicWatchNextResultsRenderer/tabbedRenderer/watchNextTabbedResultsRenderer/tabs/0/tabRenderer/content/musicQueueRenderer/content/playlistPanelRenderer");

    if let Some(panel) = panel {
        // Get playlist ID
        playlist_id = panel
            .get("playlistId")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Get contents (tracks)
        if let Some(contents) = panel.get("contents").and_then(|c| c.as_array()) {
            for item in contents {
                if let Some(track) = parse_radio_track(item, seed_video_id) {
                    tracks.push(track);
                }
            }
        }
    }

    // Alternative path for some responses
    if tracks.is_empty()
        && let Some(automix) = response.pointer("/contents/singleColumnMusicWatchNextResultsRenderer/tabbedRenderer/watchNextTabbedResultsRenderer/tabs/0/tabRenderer/content/musicQueueRenderer/content/playlistPanelRenderer/contents")
        && let Some(contents) = automix.as_array()
    {
        for item in contents {
            if let Some(track) = parse_radio_track(item, seed_video_id) {
                tracks.push(track);
            }
        }
    }

    Ok(RadioResponse {
        tracks,
        playlist_id,
    })
}

/// Parse a single track from the radio response.
fn parse_radio_track(item: &Value, seed_video_id: &str) -> Option<RadioTrack> {
    let renderer = item.get("playlistPanelVideoRenderer")?;

    // Get video ID
    let video_id = renderer
        .get("videoId")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Skip the seed track
    if video_id == seed_video_id {
        return None;
    }

    // Get title
    let title = renderer
        .pointer("/title/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Parse artists from shortBylineText or longBylineText
    let mut artists = Vec::new();
    let byline_runs = renderer
        .pointer("/shortBylineText/runs")
        .or_else(|| renderer.pointer("/longBylineText/runs"))
        .and_then(|v| v.as_array());

    if let Some(runs) = byline_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");

            // Skip separators
            if text == " • " || text == " & " || text == ", " || text == " · " {
                continue;
            }

            let browse_id = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str())
                .map(String::from);

            // Only add if it looks like an artist (has UC prefix or no browse ID but is text)
            if let Some(ref id) = browse_id {
                if id.starts_with("UC") {
                    artists.push(ArtistRef {
                        name: text.to_string(),
                        browse_id: Some(id.clone()),
                    });
                }
            } else if !text.is_empty() && !text.contains(':') {
                artists.push(ArtistRef {
                    name: text.to_string(),
                    browse_id: None,
                });
            }
        }
    }

    // Parse album if available
    let album = renderer
        .pointer("/longBylineText/runs")
        .and_then(|v| v.as_array())
        .and_then(|runs| {
            runs.iter().find_map(|run| {
                let browse_id = run
                    .pointer("/navigationEndpoint/browseEndpoint/browseId")
                    .and_then(|v| v.as_str())?;

                if browse_id.starts_with("MPREb") {
                    let title = run.get("text").and_then(|v| v.as_str())?;
                    Some(AlbumRef {
                        title: title.to_string(),
                        browse_id: Some(browse_id.to_string()),
                    })
                } else {
                    None
                }
            })
        });

    // Parse duration
    let duration_seconds = renderer
        .pointer("/lengthText/runs/0/text")
        .and_then(|v| v.as_str())
        .and_then(parse_duration);

    // Get thumbnail
    let thumbnail_url = renderer
        .pointer("/thumbnail/thumbnails/0/url")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Check for explicit badge
    let is_explicit = renderer
        .get("badges")
        .and_then(|v| v.as_array())
        .map(|badges| {
            badges.iter().any(|b| {
                b.pointer("/musicInlineBadgeRenderer/icon/iconType")
                    .and_then(|v| v.as_str())
                    == Some("MUSIC_EXPLICIT_BADGE")
            })
        })
        .unwrap_or(false);

    Some(RadioTrack {
        video_id,
        title,
        artists,
        album,
        duration_seconds,
        thumbnail_url,
        is_explicit,
    })
}

/// Parse a duration string (e.g., "3:45") into seconds.
fn parse_duration(duration_str: &str) -> Option<u32> {
    let parts: Vec<&str> = duration_str.split(':').collect();
    match parts.len() {
        2 => {
            let minutes: u32 = parts[0].parse().ok()?;
            let seconds: u32 = parts[1].parse().ok()?;
            Some(minutes * 60 + seconds)
        }
        3 => {
            let hours: u32 = parts[0].parse().ok()?;
            let minutes: u32 = parts[1].parse().ok()?;
            let seconds: u32 = parts[2].parse().ok()?;
            Some(hours * 3600 + minutes * 60 + seconds)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_radio_response() -> Value {
        json!({
            "contents": {
                "singleColumnMusicWatchNextResultsRenderer": {
                    "tabbedRenderer": {
                        "watchNextTabbedResultsRenderer": {
                            "tabs": [{
                                "tabRenderer": {
                                    "content": {
                                        "musicQueueRenderer": {
                                            "content": {
                                                "playlistPanelRenderer": {
                                                    "playlistId": "RDAMVMdQw4w9WgXcQ",
                                                    "contents": [
                                                        {
                                                            "playlistPanelVideoRenderer": {
                                                                "videoId": "dQw4w9WgXcQ",
                                                                "title": {
                                                                    "runs": [{"text": "Never Gonna Give You Up"}]
                                                                },
                                                                "shortBylineText": {
                                                                    "runs": [{
                                                                        "text": "Rick Astley",
                                                                        "navigationEndpoint": {
                                                                            "browseEndpoint": {
                                                                                "browseId": "UCuAXFkgsw1L7xaCfnd5JJOw"
                                                                            }
                                                                        }
                                                                    }]
                                                                },
                                                                "lengthText": {
                                                                    "runs": [{"text": "3:33"}]
                                                                },
                                                                "thumbnail": {
                                                                    "thumbnails": [{
                                                                        "url": "https://i.ytimg.com/vi/dQw4w9WgXcQ/mqdefault.jpg"
                                                                    }]
                                                                }
                                                            }
                                                        },
                                                        {
                                                            "playlistPanelVideoRenderer": {
                                                                "videoId": "abc123xyz",
                                                                "title": {
                                                                    "runs": [{"text": "Similar Track"}]
                                                                },
                                                                "shortBylineText": {
                                                                    "runs": [{
                                                                        "text": "Another Artist",
                                                                        "navigationEndpoint": {
                                                                            "browseEndpoint": {
                                                                                "browseId": "UCtest123"
                                                                            }
                                                                        }
                                                                    }]
                                                                },
                                                                "lengthText": {
                                                                    "runs": [{"text": "4:20"}]
                                                                }
                                                            }
                                                        }
                                                    ]
                                                }
                                            }
                                        }
                                    }
                                }
                            }]
                        }
                    }
                }
            }
        })
    }

    #[test]
    fn test_parse_radio_response() {
        let response = mock_radio_response();
        let result = parse_radio_response(&response, "dQw4w9WgXcQ").unwrap();

        // Should skip the seed track and only return the similar track
        assert_eq!(result.tracks.len(), 1);

        let track = &result.tracks[0];
        assert_eq!(track.video_id, "abc123xyz");
        assert_eq!(track.title, "Similar Track");
        assert_eq!(track.artists.len(), 1);
        assert_eq!(track.artists[0].name, "Another Artist");
        assert_eq!(track.duration_seconds, Some(260)); // 4:20 = 260 seconds

        assert_eq!(result.playlist_id, Some("RDAMVMdQw4w9WgXcQ".to_string()));
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("3:45"), Some(225));
        assert_eq!(parse_duration("0:30"), Some(30));
        assert_eq!(parse_duration("1:00:00"), Some(3600));
        assert_eq!(parse_duration("1:30:45"), Some(5445));
        assert_eq!(parse_duration("invalid"), None);
    }

    #[test]
    fn test_empty_response() {
        let empty_response = json!({});
        let result = parse_radio_response(&empty_response, "test").unwrap();
        assert!(result.tracks.is_empty());
        assert!(result.playlist_id.is_none());
    }
}
