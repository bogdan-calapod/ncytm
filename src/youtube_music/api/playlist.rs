//! YouTube Music playlist API.
//!
//! Provides access to playlist details and tracks.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::library::LibraryTrack;
use super::search::ArtistRef;
use crate::youtube_music::{ClientError, YouTubeMusicClient};

/// Full playlist details with tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    /// Playlist ID.
    pub playlist_id: String,
    /// Playlist title.
    pub title: String,
    /// Playlist description (if any).
    pub description: Option<String>,
    /// Author/creator name.
    pub author: Option<String>,
    /// Author channel ID.
    pub author_id: Option<String>,
    /// Total track count.
    pub track_count: Option<u32>,
    /// Total duration (formatted string).
    pub duration: Option<String>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Privacy status.
    pub privacy: Option<String>,
    /// Tracks in the playlist.
    pub tracks: Vec<LibraryTrack>,
}

/// Response for playlist with optional continuation.
#[derive(Debug, Clone)]
pub struct PlaylistResponse {
    /// The playlist with tracks loaded so far.
    pub playlist: Playlist,
    /// Continuation token for fetching more tracks.
    pub continuation: Option<String>,
}

/// Get playlist details and tracks.
///
/// # Arguments
///
/// * `client` - The YouTube Music API client
/// * `playlist_id` - The playlist ID (with or without "VL" prefix)
///
/// # Returns
///
/// Playlist details with tracks.
pub async fn get_playlist(
    client: &YouTubeMusicClient,
    playlist_id: &str,
) -> Result<PlaylistResponse, ClientError> {
    // Ensure playlist ID has VL prefix for browse endpoint
    let browse_id = if playlist_id.starts_with("VL") {
        playlist_id.to_string()
    } else {
        format!("VL{}", playlist_id)
    };

    let body = json!({
        "browseId": browse_id
    });

    let response = client.post("browse", &body).await?;
    parse_playlist_response(&response, playlist_id)
}

/// Get more tracks from a playlist using continuation token.
///
/// # Arguments
///
/// * `client` - The YouTube Music API client
/// * `continuation` - The continuation token
/// * `existing_playlist` - The existing playlist to append tracks to
///
/// # Returns
///
/// Updated playlist response with more tracks.
pub async fn get_playlist_continuation(
    client: &YouTubeMusicClient,
    continuation: &str,
    existing_playlist: Playlist,
) -> Result<PlaylistResponse, ClientError> {
    let body = json!({
        "continuation": continuation
    });

    let response = client.post("browse", &body).await?;
    parse_playlist_continuation(&response, existing_playlist)
}

/// Parse the initial playlist response.
fn parse_playlist_response(
    response: &Value,
    playlist_id: &str,
) -> Result<PlaylistResponse, ClientError> {
    // Extract header info
    let header = response.pointer("/header/musicDetailHeaderRenderer")
        .or_else(|| response.pointer("/header/musicEditablePlaylistDetailHeaderRenderer/header/musicDetailHeaderRenderer"));

    let title = header
        .and_then(|h| h.pointer("/title/runs/0/text"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| "Unknown Playlist".to_string());

    let description = header
        .and_then(|h| h.pointer("/description/runs/0/text"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let author = header
        .and_then(|h| h.pointer("/subtitle/runs/2/text"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let author_id = header
        .and_then(|h| h.pointer("/subtitle/runs/2/navigationEndpoint/browseEndpoint/browseId"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Parse track count from subtitle (e.g., "50 songs")
    let track_count = header
        .and_then(|h| h.get("secondSubtitle"))
        .and_then(|s| s.pointer("/runs/0/text"))
        .and_then(|v| v.as_str())
        .and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|n| n.replace(',', "").parse().ok())
        });

    // Parse duration from subtitle
    let duration = header
        .and_then(|h| h.get("secondSubtitle"))
        .and_then(|s| s.pointer("/runs/2/text"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Get thumbnail
    let thumbnail_url = header
        .and_then(|h| {
            h.pointer("/thumbnail/croppedSquareThumbnailRenderer/thumbnail/thumbnails/0/url")
        })
        .and_then(|v| v.as_str())
        .map(String::from);

    // Parse privacy
    let privacy = response
        .pointer("/header/musicEditablePlaylistDetailHeaderRenderer/editHeader/musicPlaylistEditHeaderRenderer/privacy")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Parse tracks
    let (tracks, continuation) = parse_playlist_tracks(response);

    Ok(PlaylistResponse {
        playlist: Playlist {
            playlist_id: playlist_id.to_string(),
            title,
            description,
            author,
            author_id,
            track_count,
            duration,
            thumbnail_url,
            privacy,
            tracks,
        },
        continuation,
    })
}

/// Parse playlist continuation response.
fn parse_playlist_continuation(
    response: &Value,
    mut existing_playlist: Playlist,
) -> Result<PlaylistResponse, ClientError> {
    let (new_tracks, continuation) = if let Some(cont) = response.get("continuationContents") {
        let shelf = cont.get("musicPlaylistShelfContinuation");
        let items = shelf
            .and_then(|s| s.get("contents"))
            .and_then(|c| c.as_array());

        let token = shelf
            .and_then(|s| s.get("continuations"))
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.pointer("/nextContinuationData/continuation"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let tracks = items
            .map(|items| items.iter().filter_map(parse_playlist_track).collect())
            .unwrap_or_default();

        (tracks, token)
    } else {
        (Vec::new(), None)
    };

    existing_playlist.tracks.extend(new_tracks);

    Ok(PlaylistResponse {
        playlist: existing_playlist,
        continuation,
    })
}

/// Parse tracks from playlist response.
fn parse_playlist_tracks(response: &Value) -> (Vec<LibraryTrack>, Option<String>) {
    // Find the music shelf with tracks
    let shelf = response
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/musicPlaylistShelfRenderer");

    let items = shelf
        .and_then(|s| s.get("contents"))
        .and_then(|c| c.as_array());

    let continuation = shelf
        .and_then(|s| s.get("continuations"))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.pointer("/nextContinuationData/continuation"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let tracks = items
        .map(|items| items.iter().filter_map(parse_playlist_track).collect())
        .unwrap_or_default();

    (tracks, continuation)
}

/// Parse a single track from playlist contents.
fn parse_playlist_track(item: &Value) -> Option<LibraryTrack> {
    let renderer = item.get("musicResponsiveListItemRenderer")?;

    // Get video ID
    let video_id = renderer
        .pointer("/playlistItemData/videoId")
        .or_else(|| renderer.pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId"))
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Get set video ID
    let set_video_id = renderer
        .pointer("/playlistItemData/playlistSetVideoId")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Get flex columns
    let flex_columns = renderer.get("flexColumns")?.as_array()?;

    // First column: title
    let title = flex_columns
        .first()?
        .pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Second column: artist info
    let second_column_runs = flex_columns
        .get(1)
        .and_then(|col| col.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
        .and_then(|v| v.as_array());

    let mut artists = Vec::new();

    if let Some(runs) = second_column_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");

            if text == " • " || text == " & " || text == ", " || text == " · " {
                continue;
            }

            let browse_id = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str())
                .map(String::from);

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

    // Third column: album (if present)
    let album = flex_columns
        .get(2)
        .and_then(|col| col.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs/0"))
        .and_then(|run| {
            let title = run.get("text").and_then(|v| v.as_str()).map(String::from)?;
            let browse_id = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str())
                .map(String::from);
            Some(super::search::AlbumRef { title, browse_id })
        });

    // Fixed column for duration (usually last)
    let fixed_columns = renderer.get("fixedColumns").and_then(|f| f.as_array());
    let duration_seconds = fixed_columns
        .and_then(|cols| cols.first())
        .and_then(|col| col.pointer("/musicResponsiveListItemFixedColumnRenderer/text/runs/0/text"))
        .and_then(|v| v.as_str())
        .and_then(parse_duration);

    // Get thumbnail
    let thumbnail_url = renderer
        .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails/0/url")
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

    Some(LibraryTrack {
        video_id,
        title,
        artists,
        album,
        duration_seconds,
        thumbnail_url,
        is_explicit,
        set_video_id,
    })
}

/// Parse a duration string into seconds.
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

    fn mock_playlist_response() -> Value {
        json!({
            "header": {
                "musicDetailHeaderRenderer": {
                    "title": {
                        "runs": [{"text": "My Awesome Playlist"}]
                    },
                    "description": {
                        "runs": [{"text": "A collection of great songs"}]
                    },
                    "subtitle": {
                        "runs": [
                            {"text": "Playlist"},
                            {"text": " • "},
                            {
                                "text": "John Doe",
                                "navigationEndpoint": {
                                    "browseEndpoint": {
                                        "browseId": "UC1234567890"
                                    }
                                }
                            }
                        ]
                    },
                    "secondSubtitle": {
                        "runs": [
                            {"text": "50 songs"},
                            {"text": " • "},
                            {"text": "3 hours, 25 minutes"}
                        ]
                    },
                    "thumbnail": {
                        "croppedSquareThumbnailRenderer": {
                            "thumbnail": {
                                "thumbnails": [{
                                    "url": "https://example.com/playlist.jpg"
                                }]
                            }
                        }
                    }
                }
            },
            "contents": {
                "singleColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicPlaylistShelfRenderer": {
                                            "contents": [{
                                                "musicResponsiveListItemRenderer": {
                                                    "playlistItemData": {
                                                        "videoId": "track1id",
                                                        "playlistSetVideoId": "set1"
                                                    },
                                                    "flexColumns": [
                                                        {
                                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                                "text": {
                                                                    "runs": [{"text": "First Track"}]
                                                                }
                                                            }
                                                        },
                                                        {
                                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                                "text": {
                                                                    "runs": [{
                                                                        "text": "Artist One",
                                                                        "navigationEndpoint": {
                                                                            "browseEndpoint": {
                                                                                "browseId": "UCartist1"
                                                                            }
                                                                        }
                                                                    }]
                                                                }
                                                            }
                                                        },
                                                        {
                                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                                "text": {
                                                                    "runs": [{
                                                                        "text": "Album One",
                                                                        "navigationEndpoint": {
                                                                            "browseEndpoint": {
                                                                                "browseId": "MPREb_album1"
                                                                            }
                                                                        }
                                                                    }]
                                                                }
                                                            }
                                                        }
                                                    ],
                                                    "fixedColumns": [{
                                                        "musicResponsiveListItemFixedColumnRenderer": {
                                                            "text": {
                                                                "runs": [{"text": "4:30"}]
                                                            }
                                                        }
                                                    }],
                                                    "thumbnail": {
                                                        "musicThumbnailRenderer": {
                                                            "thumbnail": {
                                                                "thumbnails": [{
                                                                    "url": "https://example.com/track1.jpg"
                                                                }]
                                                            }
                                                        }
                                                    }
                                                }
                                            }],
                                            "continuations": [{
                                                "nextContinuationData": {
                                                    "continuation": "playlist_cont_token"
                                                }
                                            }]
                                        }
                                    }]
                                }
                            }
                        }
                    }]
                }
            }
        })
    }

    fn mock_playlist_continuation_response() -> Value {
        json!({
            "continuationContents": {
                "musicPlaylistShelfContinuation": {
                    "contents": [{
                        "musicResponsiveListItemRenderer": {
                            "playlistItemData": {
                                "videoId": "track2id"
                            },
                            "flexColumns": [
                                {
                                    "musicResponsiveListItemFlexColumnRenderer": {
                                        "text": {
                                            "runs": [{"text": "Second Track"}]
                                        }
                                    }
                                },
                                {
                                    "musicResponsiveListItemFlexColumnRenderer": {
                                        "text": {
                                            "runs": [{"text": "Artist Two"}]
                                        }
                                    }
                                }
                            ],
                            "fixedColumns": [{
                                "musicResponsiveListItemFixedColumnRenderer": {
                                    "text": {
                                        "runs": [{"text": "3:15"}]
                                    }
                                }
                            }]
                        }
                    }]
                }
            }
        })
    }

    fn mock_empty_playlist_response() -> Value {
        json!({
            "header": {
                "musicDetailHeaderRenderer": {
                    "title": {
                        "runs": [{"text": "Empty Playlist"}]
                    }
                }
            },
            "contents": {
                "singleColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicPlaylistShelfRenderer": {
                                            "contents": []
                                        }
                                    }]
                                }
                            }
                        }
                    }]
                }
            }
        })
    }

    #[test]
    fn test_parse_playlist_header() {
        let response = mock_playlist_response();
        let result = parse_playlist_response(&response, "test_playlist_id").unwrap();
        let playlist = result.playlist;

        assert_eq!(playlist.playlist_id, "test_playlist_id");
        assert_eq!(playlist.title, "My Awesome Playlist");
        assert_eq!(
            playlist.description,
            Some("A collection of great songs".to_string())
        );
        assert_eq!(playlist.author, Some("John Doe".to_string()));
        assert_eq!(playlist.author_id, Some("UC1234567890".to_string()));
        assert_eq!(playlist.track_count, Some(50));
        assert_eq!(playlist.duration, Some("3 hours, 25 minutes".to_string()));
        assert!(playlist.thumbnail_url.is_some());
    }

    #[test]
    fn test_parse_playlist_tracks() {
        let response = mock_playlist_response();
        let result = parse_playlist_response(&response, "test_id").unwrap();
        let playlist = result.playlist;

        assert_eq!(playlist.tracks.len(), 1);
        let track = &playlist.tracks[0];
        assert_eq!(track.video_id, "track1id");
        assert_eq!(track.title, "First Track");
        assert_eq!(track.artists.len(), 1);
        assert_eq!(track.artists[0].name, "Artist One");
        assert!(track.album.is_some());
        assert_eq!(track.album.as_ref().unwrap().title, "Album One");
        assert_eq!(track.duration_seconds, Some(270)); // 4:30
        assert_eq!(track.set_video_id, Some("set1".to_string()));
    }

    #[test]
    fn test_parse_playlist_continuation_token() {
        let response = mock_playlist_response();
        let result = parse_playlist_response(&response, "test_id").unwrap();

        assert_eq!(result.continuation, Some("playlist_cont_token".to_string()));
    }

    #[test]
    fn test_parse_playlist_continuation_response() {
        let response = mock_playlist_continuation_response();
        let existing = Playlist {
            playlist_id: "test_id".to_string(),
            title: "Test".to_string(),
            description: None,
            author: None,
            author_id: None,
            track_count: None,
            duration: None,
            thumbnail_url: None,
            privacy: None,
            tracks: vec![LibraryTrack {
                video_id: "track1id".to_string(),
                title: "First Track".to_string(),
                artists: vec![],
                album: None,
                duration_seconds: None,
                thumbnail_url: None,
                is_explicit: false,
                set_video_id: None,
            }],
        };

        let result = parse_playlist_continuation(&response, existing).unwrap();

        assert_eq!(result.playlist.tracks.len(), 2);
        assert_eq!(result.playlist.tracks[0].video_id, "track1id");
        assert_eq!(result.playlist.tracks[1].video_id, "track2id");
        assert_eq!(result.playlist.tracks[1].title, "Second Track");
        assert_eq!(result.playlist.tracks[1].duration_seconds, Some(195)); // 3:15
    }

    #[test]
    fn test_parse_empty_playlist() {
        let response = mock_empty_playlist_response();
        let result = parse_playlist_response(&response, "empty_id").unwrap();

        assert_eq!(result.playlist.title, "Empty Playlist");
        assert!(result.playlist.tracks.is_empty());
        assert!(result.continuation.is_none());
    }

    #[test]
    fn test_playlist_serialization() {
        let playlist = Playlist {
            playlist_id: "test123".to_string(),
            title: "Test Playlist".to_string(),
            description: Some("Description".to_string()),
            author: Some("Author".to_string()),
            author_id: Some("UC123".to_string()),
            track_count: Some(10),
            duration: Some("30 minutes".to_string()),
            thumbnail_url: None,
            privacy: Some("PUBLIC".to_string()),
            tracks: vec![],
        };

        let json = serde_json::to_string(&playlist).unwrap();
        let deserialized: Playlist = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.playlist_id, playlist.playlist_id);
        assert_eq!(deserialized.title, playlist.title);
        assert_eq!(deserialized.track_count, playlist.track_count);
    }
}
