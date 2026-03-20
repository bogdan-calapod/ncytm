//! YouTube Music album API.
//!
//! Provides access to album details and tracks.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::search::{AlbumRef, ArtistRef};
use crate::youtube_music::{ClientError, YouTubeMusicClient};

/// Full album details with tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    /// Album browse ID.
    pub browse_id: String,
    /// Album title.
    pub title: String,
    /// Album type (Album, EP, Single).
    pub album_type: Option<String>,
    /// Artists.
    pub artists: Vec<ArtistRef>,
    /// Release year.
    pub year: Option<String>,
    /// Total track count.
    pub track_count: Option<u32>,
    /// Total duration (formatted string).
    pub duration: Option<String>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Description (if any).
    pub description: Option<String>,
    /// Whether the album is explicit.
    pub is_explicit: bool,
    /// Tracks in the album.
    pub tracks: Vec<AlbumTrack>,
    /// Audio playlist ID (for playback).
    pub audio_playlist_id: Option<String>,
}

/// A track within an album.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumTrack {
    /// YouTube video ID.
    pub video_id: String,
    /// Track title.
    pub title: String,
    /// Track number.
    pub track_number: Option<u32>,
    /// Artists (may differ from album artists for features).
    pub artists: Vec<ArtistRef>,
    /// Duration in seconds.
    pub duration_seconds: Option<u32>,
    /// Whether this is an explicit track.
    pub is_explicit: bool,
    /// Album reference (for linking back).
    pub album: Option<AlbumRef>,
}

/// Get album details and tracks.
///
/// # Arguments
///
/// * `client` - The YouTube Music API client
/// * `browse_id` - The album browse ID (usually starts with "MPREb_")
///
/// # Returns
///
/// Album details with tracks.
pub async fn get_album(
    client: &YouTubeMusicClient,
    browse_id: &str,
) -> Result<Album, ClientError> {
    let body = json!({
        "browseId": browse_id
    });

    let response = client.post("browse", &body).await?;
    parse_album_response(&response, browse_id)
}

/// Parse the album response.
fn parse_album_response(response: &Value, browse_id: &str) -> Result<Album, ClientError> {
    // Extract header info
    let header = response.pointer("/header/musicDetailHeaderRenderer");

    let title = header
        .and_then(|h| h.pointer("/title/runs/0/text"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| "Unknown Album".to_string());

    // Parse subtitle for album type, artist, year
    let subtitle_runs = header
        .and_then(|h| h.pointer("/subtitle/runs"))
        .and_then(|v| v.as_array());

    let mut album_type: Option<String> = None;
    let mut artists: Vec<ArtistRef> = Vec::new();
    let mut year: Option<String> = None;

    if let Some(runs) = subtitle_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");

            // Skip separators
            if text == " • " || text == " · " {
                continue;
            }

            // Check for album type
            if text == "Album" || text == "EP" || text == "Single" {
                album_type = Some(text.to_string());
                continue;
            }

            // Check for year
            if text.len() == 4 && text.chars().all(|c| c.is_ascii_digit()) {
                year = Some(text.to_string());
                continue;
            }

            // Check for artist navigation
            let artist_browse_id = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str())
                .map(String::from);

            if artist_browse_id.is_some() || (!text.is_empty() && !text.starts_with(' ')) {
                // Only add if not already added and looks like an artist name
                if !text.is_empty() && artists.iter().all(|a| a.name != text) {
                    artists.push(ArtistRef {
                        name: text.to_string(),
                        browse_id: artist_browse_id,
                    });
                }
            }
        }
    }

    // Parse second subtitle for track count and duration
    let second_subtitle_runs = header
        .and_then(|h| h.pointer("/secondSubtitle/runs"))
        .and_then(|v| v.as_array());

    let mut track_count: Option<u32> = None;
    let mut duration: Option<String> = None;

    if let Some(runs) = second_subtitle_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");

            if text == " • " || text == " · " {
                continue;
            }

            // Check for track count (e.g., "12 songs")
            if text.to_lowercase().contains("song") {
                track_count = text
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.replace(',', "").parse().ok());
                continue;
            }

            // Otherwise assume it's duration
            if text.contains("minute") || text.contains("hour") {
                duration = Some(text.to_string());
            }
        }
    }

    // Get thumbnail
    let thumbnail_url = header
        .and_then(|h| h.pointer("/thumbnail/croppedSquareThumbnailRenderer/thumbnail/thumbnails/0/url"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Get description
    let description = header
        .and_then(|h| h.pointer("/description/runs/0/text"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Check for explicit badge in header menu
    let is_explicit = header
        .and_then(|h| h.get("subtitleBadges"))
        .and_then(|v| v.as_array())
        .map(|badges| {
            badges.iter().any(|b| {
                b.pointer("/musicInlineBadgeRenderer/icon/iconType")
                    .and_then(|v| v.as_str())
                    == Some("MUSIC_EXPLICIT_BADGE")
            })
        })
        .unwrap_or(false);

    // Get audio playlist ID for playback
    let audio_playlist_id = header
        .and_then(|h| h.pointer("/menu/menuRenderer/items"))
        .and_then(|items| items.as_array())
        .and_then(|items| {
            items.iter().find_map(|item| {
                item.pointer("/menuNavigationItemRenderer/navigationEndpoint/watchPlaylistEndpoint/playlistId")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
        });

    // Parse tracks
    let tracks = parse_album_tracks(response, browse_id, &title);

    Ok(Album {
        browse_id: browse_id.to_string(),
        title,
        album_type,
        artists,
        year,
        track_count,
        duration,
        thumbnail_url,
        description,
        is_explicit,
        tracks,
        audio_playlist_id,
    })
}

/// Parse tracks from album response.
fn parse_album_tracks(response: &Value, album_browse_id: &str, album_title: &str) -> Vec<AlbumTrack> {
    let shelf = response
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/musicShelfRenderer");

    let items = shelf.and_then(|s| s.get("contents")).and_then(|c| c.as_array());

    let Some(items) = items else {
        return Vec::new();
    };

    let mut tracks = Vec::new();
    let mut track_number: u32 = 1;

    for item in items {
        if let Some(track) = parse_album_track(item, album_browse_id, album_title, track_number) {
            tracks.push(track);
            track_number += 1;
        }
    }

    tracks
}

/// Parse a single track from album contents.
fn parse_album_track(
    item: &Value,
    album_browse_id: &str,
    album_title: &str,
    track_number: u32,
) -> Option<AlbumTrack> {
    let renderer = item.get("musicResponsiveListItemRenderer")?;

    // Get video ID
    let video_id = renderer
        .pointer("/playlistItemData/videoId")
        .or_else(|| renderer.pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId"))
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Get flex columns
    let flex_columns = renderer.get("flexColumns")?.as_array()?;

    // First column: title
    let title = flex_columns
        .first()?
        .pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Second column: artists (may include featured artists)
    let second_column_runs = flex_columns
        .get(1)
        .and_then(|col| col.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
        .and_then(|v| v.as_array());

    let mut artists = Vec::new();

    if let Some(runs) = second_column_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");

            if text == " & " || text == ", " || text.is_empty() {
                continue;
            }

            let browse_id = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str())
                .map(String::from);

            artists.push(ArtistRef {
                name: text.to_string(),
                browse_id,
            });
        }
    }

    // Fixed column: duration
    let fixed_columns = renderer.get("fixedColumns").and_then(|f| f.as_array());
    let duration_seconds = fixed_columns
        .and_then(|cols| cols.first())
        .and_then(|col| col.pointer("/musicResponsiveListItemFixedColumnRenderer/text/runs/0/text"))
        .and_then(|v| v.as_str())
        .and_then(parse_duration);

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

    Some(AlbumTrack {
        video_id,
        title,
        track_number: Some(track_number),
        artists,
        duration_seconds,
        is_explicit,
        album: Some(AlbumRef {
            title: album_title.to_string(),
            browse_id: Some(album_browse_id.to_string()),
        }),
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

    fn mock_album_response() -> Value {
        json!({
            "header": {
                "musicDetailHeaderRenderer": {
                    "title": {
                        "runs": [{"text": "Whenever You Need Somebody"}]
                    },
                    "subtitle": {
                        "runs": [
                            {"text": "Album"},
                            {"text": " • "},
                            {
                                "text": "Rick Astley",
                                "navigationEndpoint": {
                                    "browseEndpoint": {
                                        "browseId": "UCuAXFkgsw1L7xaCfnd5JJOw"
                                    }
                                }
                            },
                            {"text": " • "},
                            {"text": "1987"}
                        ]
                    },
                    "secondSubtitle": {
                        "runs": [
                            {"text": "10 songs"},
                            {"text": " • "},
                            {"text": "42 minutes"}
                        ]
                    },
                    "thumbnail": {
                        "croppedSquareThumbnailRenderer": {
                            "thumbnail": {
                                "thumbnails": [{
                                    "url": "https://example.com/album.jpg"
                                }]
                            }
                        }
                    },
                    "description": {
                        "runs": [{"text": "Rick Astley's debut album"}]
                    },
                    "menu": {
                        "menuRenderer": {
                            "items": [{
                                "menuNavigationItemRenderer": {
                                    "navigationEndpoint": {
                                        "watchPlaylistEndpoint": {
                                            "playlistId": "OLAK5uy_abc123"
                                        }
                                    }
                                }
                            }]
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
                                        "musicShelfRenderer": {
                                            "contents": [
                                                {
                                                    "musicResponsiveListItemRenderer": {
                                                        "playlistItemData": {
                                                            "videoId": "dQw4w9WgXcQ"
                                                        },
                                                        "flexColumns": [
                                                            {
                                                                "musicResponsiveListItemFlexColumnRenderer": {
                                                                    "text": {
                                                                        "runs": [{"text": "Never Gonna Give You Up"}]
                                                                    }
                                                                }
                                                            },
                                                            {
                                                                "musicResponsiveListItemFlexColumnRenderer": {
                                                                    "text": {
                                                                        "runs": [{
                                                                            "text": "Rick Astley",
                                                                            "navigationEndpoint": {
                                                                                "browseEndpoint": {
                                                                                    "browseId": "UCuAXFkgsw1L7xaCfnd5JJOw"
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
                                                                    "runs": [{"text": "3:33"}]
                                                                }
                                                            }
                                                        }]
                                                    }
                                                },
                                                {
                                                    "musicResponsiveListItemRenderer": {
                                                        "playlistItemData": {
                                                            "videoId": "track2id"
                                                        },
                                                        "flexColumns": [
                                                            {
                                                                "musicResponsiveListItemFlexColumnRenderer": {
                                                                    "text": {
                                                                        "runs": [{"text": "Together Forever"}]
                                                                    }
                                                                }
                                                            },
                                                            {
                                                                "musicResponsiveListItemFlexColumnRenderer": {
                                                                    "text": {
                                                                        "runs": [{"text": "Rick Astley"}]
                                                                    }
                                                                }
                                                            }
                                                        ],
                                                        "fixedColumns": [{
                                                            "musicResponsiveListItemFixedColumnRenderer": {
                                                                "text": {
                                                                    "runs": [{"text": "3:24"}]
                                                                }
                                                            }
                                                        }]
                                                    }
                                                }
                                            ]
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

    fn mock_empty_album_response() -> Value {
        json!({
            "header": {
                "musicDetailHeaderRenderer": {
                    "title": {
                        "runs": [{"text": "Empty Album"}]
                    }
                }
            },
            "contents": {
                "singleColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": []
                                }
                            }
                        }
                    }]
                }
            }
        })
    }

    #[test]
    fn test_parse_album_header() {
        let response = mock_album_response();
        let album = parse_album_response(&response, "MPREb_test123").unwrap();

        assert_eq!(album.browse_id, "MPREb_test123");
        assert_eq!(album.title, "Whenever You Need Somebody");
        assert_eq!(album.album_type, Some("Album".to_string()));
        assert_eq!(album.artists.len(), 1);
        assert_eq!(album.artists[0].name, "Rick Astley");
        assert_eq!(album.year, Some("1987".to_string()));
        assert_eq!(album.track_count, Some(10));
        assert_eq!(album.duration, Some("42 minutes".to_string()));
        assert!(album.thumbnail_url.is_some());
        assert_eq!(album.description, Some("Rick Astley's debut album".to_string()));
        assert_eq!(album.audio_playlist_id, Some("OLAK5uy_abc123".to_string()));
    }

    #[test]
    fn test_parse_album_tracks() {
        let response = mock_album_response();
        let album = parse_album_response(&response, "MPREb_test123").unwrap();

        assert_eq!(album.tracks.len(), 2);

        let track1 = &album.tracks[0];
        assert_eq!(track1.video_id, "dQw4w9WgXcQ");
        assert_eq!(track1.title, "Never Gonna Give You Up");
        assert_eq!(track1.track_number, Some(1));
        assert_eq!(track1.duration_seconds, Some(213)); // 3:33
        assert!(track1.album.is_some());
        assert_eq!(track1.album.as_ref().unwrap().title, "Whenever You Need Somebody");

        let track2 = &album.tracks[1];
        assert_eq!(track2.video_id, "track2id");
        assert_eq!(track2.title, "Together Forever");
        assert_eq!(track2.track_number, Some(2));
        assert_eq!(track2.duration_seconds, Some(204)); // 3:24
    }

    #[test]
    fn test_parse_empty_album() {
        let response = mock_empty_album_response();
        let album = parse_album_response(&response, "empty_id").unwrap();

        assert_eq!(album.title, "Empty Album");
        assert!(album.tracks.is_empty());
    }

    #[test]
    fn test_album_serialization() {
        let album = Album {
            browse_id: "test123".to_string(),
            title: "Test Album".to_string(),
            album_type: Some("Album".to_string()),
            artists: vec![ArtistRef {
                name: "Test Artist".to_string(),
                browse_id: Some("UC123".to_string()),
            }],
            year: Some("2024".to_string()),
            track_count: Some(12),
            duration: Some("45 minutes".to_string()),
            thumbnail_url: None,
            description: None,
            is_explicit: false,
            tracks: vec![],
            audio_playlist_id: Some("OLAK123".to_string()),
        };

        let json = serde_json::to_string(&album).unwrap();
        let deserialized: Album = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.browse_id, album.browse_id);
        assert_eq!(deserialized.title, album.title);
        assert_eq!(deserialized.audio_playlist_id, album.audio_playlist_id);
    }

    #[test]
    fn test_album_track_serialization() {
        let track = AlbumTrack {
            video_id: "abc123".to_string(),
            title: "Test Track".to_string(),
            track_number: Some(5),
            artists: vec![],
            duration_seconds: Some(240),
            is_explicit: true,
            album: None,
        };

        let json = serde_json::to_string(&track).unwrap();
        let deserialized: AlbumTrack = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.video_id, track.video_id);
        assert_eq!(deserialized.track_number, track.track_number);
        assert!(deserialized.is_explicit);
    }
}
