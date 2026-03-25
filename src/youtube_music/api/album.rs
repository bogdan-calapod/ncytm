//! YouTube Music album API.
//!
//! Provides access to album pages: tracks and album details.

use serde_json::{Value, json};

use super::library::ArtistRef;
use crate::youtube_music::{ClientError, YouTubeMusicClient};

/// Album details from an album page.
#[derive(Debug, Clone)]
pub struct AlbumDetails {
    /// Album browse ID.
    pub browse_id: String,
    /// Album title.
    pub title: String,
    /// Artist names.
    pub artists: Vec<ArtistRef>,
    /// Release year.
    pub year: Option<String>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Whether this is an explicit album.
    pub is_explicit: bool,
    /// Audio playlist ID (for playback).
    pub audio_playlist_id: Option<String>,
    /// Album type (Album, Single, EP).
    #[allow(dead_code)]
    pub album_type: Option<String>,
    /// Track count.
    #[allow(dead_code)]
    pub track_count: Option<u32>,
    /// Duration string (e.g., "45 minutes").
    #[allow(dead_code)]
    pub duration: Option<String>,
}

/// A track from an album.
#[derive(Debug, Clone)]
pub struct AlbumTrack {
    /// YouTube video ID.
    pub video_id: String,
    /// Track title.
    pub title: String,
    /// Artists on this track.
    pub artists: Vec<ArtistRef>,
    /// Duration in seconds.
    pub duration_seconds: Option<u32>,
    /// Whether this is an explicit track.
    pub is_explicit: bool,
    /// Track number (1-indexed).
    pub track_number: Option<u32>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
}

/// Full album page response.
#[derive(Debug, Clone)]
pub struct AlbumPage {
    /// Album details.
    pub details: Option<AlbumDetails>,
    /// Tracks in this album.
    pub tracks: Vec<AlbumTrack>,
}

/// Get an album's page with all its tracks.
///
/// # Arguments
///
/// * `client` - The YouTube Music API client
/// * `browse_id` - The album's browse ID (usually starts with MPREb)
///
/// # Returns
///
/// The album's page with details and tracks.
pub async fn get_album(
    client: &YouTubeMusicClient,
    browse_id: &str,
) -> Result<AlbumPage, ClientError> {
    let body = json!({
        "browseId": browse_id
    });

    let response = client.post("browse", &body).await?;
    Ok(parse_album_response(&response, browse_id))
}

/// Parse the album page response.
fn parse_album_response(response: &Value, browse_id: &str) -> AlbumPage {
    let details = parse_album_header(response, browse_id);
    let tracks = parse_album_tracks(response);

    AlbumPage { details, tracks }
}

/// Parse album header information.
fn parse_album_header(response: &Value, browse_id: &str) -> Option<AlbumDetails> {
    // Try to find header in different locations:
    //   - Classic layout: header.musicDetailHeaderRenderer
    //   - Immersive layout: header.musicImmersiveHeaderRenderer
    //   - 2024 layout: contents.twoColumnBrowseResultsRenderer.tabs[0].tabRenderer.content
    //                  .sectionListRenderer.contents[0].musicResponsiveHeaderRenderer
    let header = response
        .get("header")
        .and_then(|h| h.get("musicDetailHeaderRenderer"))
        .or_else(|| {
            response
                .get("header")
                .and_then(|h| h.get("musicImmersiveHeaderRenderer"))
        })
        .or_else(|| {
            response.pointer("/contents/twoColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/musicResponsiveHeaderRenderer")
        })?;

    // Get title
    let title = header
        .pointer("/title/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Get thumbnail
    let thumbnail_url = header
        .pointer("/thumbnail/croppedSquareThumbnailRenderer/thumbnail/thumbnails")
        .or_else(|| header.pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails"))
        .and_then(|v| v.as_array())
        .and_then(|thumbs| thumbs.last())
        .and_then(|t| t.get("url"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Parse subtitle for artists, year, album type, duration, track count
    let subtitle_runs = header.pointer("/subtitle/runs").and_then(|v| v.as_array());

    let mut artists = Vec::new();
    let mut year: Option<String> = None;
    let mut album_type: Option<String> = None;
    let mut duration: Option<String> = None;
    let mut track_count: Option<u32> = None;

    if let Some(runs) = subtitle_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");

            // Skip separators
            if text == " • " || text == " · " || text.is_empty() {
                continue;
            }

            // Check for album type
            if text == "Album" || text == "EP" || text == "Single" || text == "Playlist" {
                album_type = Some(text.to_string());
                continue;
            }

            // Check for year (4 digit number)
            if text.len() == 4 && text.chars().all(|c| c.is_ascii_digit()) {
                year = Some(text.to_string());
                continue;
            }

            // Check for track count (e.g., "10 songs", "1 song")
            if text.contains("song") {
                if let Some(num_str) = text.split_whitespace().next() {
                    track_count = num_str.parse().ok();
                }
                continue;
            }

            // Check for duration (e.g., "45 minutes", "1 hour 30 minutes")
            if text.contains("minute") || text.contains("hour") {
                duration = Some(text.to_string());
                continue;
            }

            // Check for artist browse endpoint
            let artist_browse_id = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str())
                .map(String::from);

            // Artists have UC prefix or no browse ID
            if artist_browse_id
                .as_ref()
                .is_none_or(|id| id.starts_with("UC"))
            {
                artists.push(ArtistRef {
                    name: text.to_string(),
                    browse_id: artist_browse_id,
                });
            }
        }
    }

    // Check for explicit badge in subtitleBadges
    let is_explicit = header
        .get("subtitleBadges")
        .and_then(|v| v.as_array())
        .map(|badges| {
            badges.iter().any(|b| {
                b.pointer("/musicInlineBadgeRenderer/icon/iconType")
                    .and_then(|v| v.as_str())
                    == Some("MUSIC_EXPLICIT_BADGE")
            })
        })
        .unwrap_or(false);

    // Get audio playlist ID from menu items
    let audio_playlist_id = response
        .pointer("/header/musicDetailHeaderRenderer/menu/menuRenderer/items")
        .or_else(|| {
            response.pointer("/header/musicImmersiveHeaderRenderer/menu/menuRenderer/items")
        })
        .and_then(|v| v.as_array())
        .and_then(|items| {
            items.iter().find_map(|item| {
                item.pointer(
                    "/menuNavigationItemRenderer/navigationEndpoint/watchEndpoint/playlistId",
                )
                .or_else(|| {
                    item.pointer(
                        "/menuServiceItemRenderer/serviceEndpoint/playlistEditEndpoint/playlistId",
                    )
                })
                .and_then(|v| v.as_str())
                .map(String::from)
            })
        })
        // Also try from the play button
        .or_else(|| {
            response
                .pointer("/header/musicDetailHeaderRenderer/menu/menuRenderer/topLevelButtons")
                .and_then(|v| v.as_array())
                .and_then(|buttons| {
                    buttons.iter().find_map(|btn| {
                        btn.pointer("/buttonRenderer/navigationEndpoint/watchEndpoint/playlistId")
                            .and_then(|v| v.as_str())
                            .map(String::from)
                    })
                })
        });

    Some(AlbumDetails {
        browse_id: browse_id.to_string(),
        title,
        artists,
        year,
        thumbnail_url,
        is_explicit,
        audio_playlist_id,
        album_type,
        track_count,
        duration,
    })
}

/// Parse tracks from the album response.
fn parse_album_tracks(response: &Value) -> Vec<AlbumTrack> {
    let mut tracks = Vec::new();

    // Real YTM WEB_REMIX layout: twoColumnBrowseResultsRenderer → secondaryContents.
    // Tracks are under musicShelfRenderer (albums) or musicPlaylistShelfRenderer.
    // Fall back to the legacy singleColumnBrowseResultsRenderer for older responses.
    let shelf = response
        .pointer("/contents/twoColumnBrowseResultsRenderer/secondaryContents/sectionListRenderer/contents/0/musicShelfRenderer")
        .or_else(|| {
            response.pointer("/contents/twoColumnBrowseResultsRenderer/secondaryContents/sectionListRenderer/contents/0/musicPlaylistShelfRenderer")
        })
        .or_else(|| {
            response.pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/musicShelfRenderer")
        });

    if let Some(shelf) = shelf
        && let Some(contents) = shelf.get("contents").and_then(|v| v.as_array())
    {
        for (index, item) in contents.iter().enumerate() {
            if let Some(track) = parse_album_track(item, index as u32 + 1) {
                tracks.push(track);
            }
        }
    }

    tracks
}

/// Parse a single track from an album.
fn parse_album_track(item: &Value, track_number: u32) -> Option<AlbumTrack> {
    let renderer = item.get("musicResponsiveListItemRenderer")?;

    // Get video ID — prefer the play button overlay (most reliable), then playlistItemData,
    // then the title run's watchEndpoint.
    let video_id = renderer
        .pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId")
        .or_else(|| renderer.pointer("/playlistItemData/videoId"))
        .or_else(|| {
            renderer.pointer("/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs/0/navigationEndpoint/watchEndpoint/videoId")
        })
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

    // Second column: artists
    let second_column_runs = flex_columns
        .get(1)
        .and_then(|col| col.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
        .and_then(|v| v.as_array());

    let mut artists = Vec::new();

    if let Some(runs) = second_column_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");

            // Skip separators
            if text == " & " || text == ", " || text == " · " || text.is_empty() {
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

    // Parse duration from fixedColumns
    let duration_seconds = renderer
        .get("fixedColumns")
        .and_then(|v| v.as_array())
        .and_then(|cols| {
            cols.iter().find_map(|col| {
                col.pointer("/musicResponsiveListItemFixedColumnRenderer/text/runs/0/text")
                    .and_then(|v| v.as_str())
                    .filter(|s| s.contains(':'))
                    .and_then(parse_duration)
            })
        });

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

    Some(AlbumTrack {
        video_id,
        title,
        artists,
        duration_seconds,
        is_explicit,
        track_number: Some(track_number),
        thumbnail_url,
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

    /// Mock response matching the real YouTube Music twoColumnBrowseResultsRenderer layout.
    fn mock_album_response() -> Value {
        json!({
            "header": {
                "musicDetailHeaderRenderer": {
                    "title": { "runs": [{"text": "Test Album"}] },
                    "subtitle": {
                        "runs": [
                            {"text": "Album"},
                            {"text": " • "},
                            {
                                "text": "Test Artist",
                                "navigationEndpoint": {
                                    "browseEndpoint": { "browseId": "UCtest123" }
                                }
                            },
                            {"text": " • "},
                            {"text": "2023"},
                            {"text": " • "},
                            {"text": "10 songs"},
                            {"text": " • "},
                            {"text": "45 minutes"}
                        ]
                    },
                    "thumbnail": {
                        "croppedSquareThumbnailRenderer": {
                            "thumbnail": {
                                "thumbnails": [
                                    {"url": "https://example.com/small.jpg"},
                                    {"url": "https://example.com/large.jpg"}
                                ]
                            }
                        }
                    },
                    "menu": {
                        "menuRenderer": {
                            "items": [{
                                "menuNavigationItemRenderer": {
                                    "navigationEndpoint": {
                                        "watchEndpoint": { "playlistId": "OLAK5uy_test123" }
                                    }
                                }
                            }]
                        }
                    }
                }
            },
            "contents": {
                "twoColumnBrowseResultsRenderer": {
                    "secondaryContents": {
                        "sectionListRenderer": {
                            "contents": [{
                                "musicShelfRenderer": {
                                    "contents": [
                                        {
                                            "musicResponsiveListItemRenderer": {
                                                "overlay": {
                                                    "musicItemThumbnailOverlayRenderer": {
                                                        "content": {
                                                            "musicPlayButtonRenderer": {
                                                                "playNavigationEndpoint": {
                                                                    "watchEndpoint": { "videoId": "video1" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                },
                                                "flexColumns": [
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": { "runs": [{"text": "Track One"}] }
                                                        }
                                                    },
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [{
                                                                    "text": "Test Artist",
                                                                    "navigationEndpoint": {
                                                                        "browseEndpoint": { "browseId": "UCtest123" }
                                                                    }
                                                                }]
                                                            }
                                                        }
                                                    }
                                                ],
                                                "fixedColumns": [{
                                                    "musicResponsiveListItemFixedColumnRenderer": {
                                                        "text": { "runs": [{"text": "3:30"}] }
                                                    }
                                                }]
                                            }
                                        },
                                        {
                                            "musicResponsiveListItemRenderer": {
                                                "overlay": {
                                                    "musicItemThumbnailOverlayRenderer": {
                                                        "content": {
                                                            "musicPlayButtonRenderer": {
                                                                "playNavigationEndpoint": {
                                                                    "watchEndpoint": { "videoId": "video2" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                },
                                                "flexColumns": [
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": { "runs": [{"text": "Track Two"}] }
                                                        }
                                                    },
                                                    {
                                                        "musicResponsiveListItemFlexColumnRenderer": {
                                                            "text": {
                                                                "runs": [
                                                                    {"text": "Test Artist"},
                                                                    {"text": " & "},
                                                                    {"text": "Other Artist"}
                                                                ]
                                                            }
                                                        }
                                                    }
                                                ],
                                                "fixedColumns": [{
                                                    "musicResponsiveListItemFixedColumnRenderer": {
                                                        "text": { "runs": [{"text": "4:15"}] }
                                                    }
                                                }],
                                                "badges": [{
                                                    "musicInlineBadgeRenderer": {
                                                        "icon": { "iconType": "MUSIC_EXPLICIT_BADGE" }
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
            }
        })
    }

    #[test]
    fn test_parse_album_header() {
        let response = mock_album_response();
        let details = parse_album_header(&response, "MPREb_test123").unwrap();

        assert_eq!(details.title, "Test Album");
        assert_eq!(details.browse_id, "MPREb_test123");
        assert_eq!(details.year, Some("2023".to_string()));
        assert_eq!(details.album_type, Some("Album".to_string()));
        assert_eq!(details.track_count, Some(10));
        assert_eq!(details.duration, Some("45 minutes".to_string()));
        assert_eq!(details.artists.len(), 1);
        assert_eq!(details.artists[0].name, "Test Artist");
        assert!(details.thumbnail_url.is_some());
        assert_eq!(
            details.audio_playlist_id,
            Some("OLAK5uy_test123".to_string())
        );
    }

    #[test]
    fn test_parse_album_tracks() {
        let response = mock_album_response();
        let tracks = parse_album_tracks(&response);

        assert_eq!(tracks.len(), 2);

        // First track
        assert_eq!(tracks[0].video_id, "video1");
        assert_eq!(tracks[0].title, "Track One");
        assert_eq!(tracks[0].track_number, Some(1));
        assert_eq!(tracks[0].duration_seconds, Some(210));
        assert!(!tracks[0].is_explicit);
        assert_eq!(tracks[0].artists.len(), 1);

        // Second track
        assert_eq!(tracks[1].video_id, "video2");
        assert_eq!(tracks[1].title, "Track Two");
        assert_eq!(tracks[1].track_number, Some(2));
        assert_eq!(tracks[1].duration_seconds, Some(255));
        assert!(tracks[1].is_explicit);
        assert_eq!(tracks[1].artists.len(), 2);
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("3:30"), Some(210));
        assert_eq!(parse_duration("1:00:00"), Some(3600));
        assert_eq!(parse_duration("invalid"), None);
    }
}
