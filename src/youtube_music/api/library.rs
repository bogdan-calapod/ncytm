//! YouTube Music library API.
//!
//! Provides access to the user's library: liked songs, playlists, and albums.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::youtube_music::{ClientError, YouTubeMusicClient};

/// Reference to an artist (used in tracks and albums).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistRef {
    /// Artist name.
    pub name: String,
    /// Browse ID (if available).
    pub browse_id: Option<String>,
}

/// Reference to an album (used in tracks).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumRef {
    /// Album title.
    pub title: String,
    /// Browse ID (if available).
    pub browse_id: Option<String>,
}

/// Browse ID for the user's liked songs playlist.
const LIKED_SONGS_BROWSE_ID: &str = "FEmusic_liked_videos";

/// Browse ID for the user's library playlists.
const LIBRARY_PLAYLISTS_BROWSE_ID: &str = "FEmusic_liked_playlists";

/// Browse ID for the user's library albums.
const LIBRARY_ALBUMS_BROWSE_ID: &str = "FEmusic_liked_albums";

/// A track from the user's library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryTrack {
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
    /// Set video ID (for removing from library).
    pub set_video_id: Option<String>,
}

/// A playlist from the user's library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryPlaylist {
    /// Playlist ID.
    pub playlist_id: String,
    /// Playlist title.
    pub title: String,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Number of tracks (as string, e.g., "50 songs").
    pub track_count: Option<String>,
}

/// An album from the user's library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryAlbum {
    /// Album browse ID.
    pub browse_id: String,
    /// Album title.
    pub title: String,
    /// Artist names.
    pub artists: Vec<ArtistRef>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Release year.
    pub year: Option<String>,
    /// Whether this is an explicit album.
    pub is_explicit: bool,
}

/// Response containing library items with optional continuation.
#[derive(Debug, Clone)]
pub struct LibraryResponse<T> {
    /// The items in this page.
    pub items: Vec<T>,
    /// Continuation token for fetching more items (if available).
    pub continuation: Option<String>,
}

/// Get the user's liked songs.
///
/// # Arguments
///
/// * `client` - The YouTube Music API client
/// * `continuation` - Optional continuation token for pagination
///
/// # Returns
///
/// A response containing liked tracks and an optional continuation token.
pub async fn get_liked_songs(
    client: &YouTubeMusicClient,
    continuation: Option<&str>,
) -> Result<LibraryResponse<LibraryTrack>, ClientError> {
    let response = if let Some(token) = continuation {
        // Use continuation endpoint
        let body = json!({
            "continuation": token
        });
        client.post("browse", &body).await?
    } else {
        // Initial request
        let body = json!({
            "browseId": LIKED_SONGS_BROWSE_ID
        });
        client.post("browse", &body).await?
    };

    parse_library_tracks_response(&response)
}

/// Get the user's library playlists.
///
/// # Arguments
///
/// * `client` - The YouTube Music API client
/// * `continuation` - Optional continuation token for pagination
///
/// # Returns
///
/// A response containing playlists and an optional continuation token.
pub async fn get_library_playlists(
    client: &YouTubeMusicClient,
    continuation: Option<&str>,
) -> Result<LibraryResponse<LibraryPlaylist>, ClientError> {
    let response = if let Some(token) = continuation {
        let body = json!({
            "continuation": token
        });
        client.post("browse", &body).await?
    } else {
        let body = json!({
            "browseId": LIBRARY_PLAYLISTS_BROWSE_ID
        });
        client.post("browse", &body).await?
    };

    parse_library_playlists_response(&response)
}

/// Get the user's library albums.
///
/// # Arguments
///
/// * `client` - The YouTube Music API client
/// * `continuation` - Optional continuation token for pagination
///
/// # Returns
///
/// A response containing albums and an optional continuation token.
pub async fn get_library_albums(
    client: &YouTubeMusicClient,
    continuation: Option<&str>,
) -> Result<LibraryResponse<LibraryAlbum>, ClientError> {
    let response = if let Some(token) = continuation {
        let body = json!({
            "continuation": token
        });
        client.post("browse", &body).await?
    } else {
        let body = json!({
            "browseId": LIBRARY_ALBUMS_BROWSE_ID
        });
        client.post("browse", &body).await?
    };

    parse_library_albums_response(&response)
}

/// Parse the liked songs response.
fn parse_library_tracks_response(
    response: &Value,
) -> Result<LibraryResponse<LibraryTrack>, ClientError> {
    let mut tracks = Vec::new();

    // Try to find contents in the response
    // Initial response: contents.singleColumnBrowseResultsRenderer.tabs[0].tabRenderer.content.sectionListRenderer.contents[0].musicShelfRenderer
    // Continuation response: continuationContents.musicShelfContinuation

    let (contents, cont_token) = if let Some(cont) = response.get("continuationContents") {
        let shelf = cont.get("musicShelfContinuation");
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
        (items, token)
    } else {
        // Initial response
        let shelf = response
            .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/musicShelfRenderer");
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
        (items, token)
    };

    if let Some(items) = contents {
        for item in items {
            if let Some(track) = parse_library_track(item) {
                tracks.push(track);
            }
        }
    }

    Ok(LibraryResponse {
        items: tracks,
        continuation: cont_token,
    })
}

/// Parse a single library track from a musicResponsiveListItemRenderer.
fn parse_library_track(item: &Value) -> Option<LibraryTrack> {
    let renderer = item.get("musicResponsiveListItemRenderer")?;

    // Get video ID
    let video_id = renderer
        .pointer("/playlistItemData/videoId")
        .or_else(|| renderer.pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId"))
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Get set video ID (for library management)
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

    // Second column: artist - album - duration
    let second_column_runs = flex_columns
        .get(1)
        .and_then(|col| col.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
        .and_then(|v| v.as_array());

    let mut artists = Vec::new();
    let mut album: Option<AlbumRef> = None;
    let mut duration_seconds: Option<u32> = None;

    // Check for duration in fixedColumns (YouTube Music puts duration there)
    if let Some(fixed_columns) = renderer.get("fixedColumns").and_then(|v| v.as_array()) {
        for col in fixed_columns {
            if let Some(text) = col
                .pointer("/musicResponsiveListItemFixedColumnRenderer/text/runs/0/text")
                .and_then(|v| v.as_str())
                && text.contains(':')
            {
                duration_seconds = parse_duration(text);
            }
        }
    }

    if let Some(runs) = second_column_runs {
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

            if let Some(ref id) = browse_id {
                if id.starts_with("UC") {
                    artists.push(ArtistRef {
                        name: text.to_string(),
                        browse_id: Some(id.clone()),
                    });
                    continue;
                } else if id.starts_with("MPREb") {
                    album = Some(AlbumRef {
                        title: text.to_string(),
                        browse_id: Some(id.clone()),
                    });
                    continue;
                }
            }

            // Parse duration
            if text.contains(':') {
                duration_seconds = parse_duration(text);
                continue;
            }

            // Assume artist if no browse_id
            if browse_id.is_none() && !text.is_empty() && !text.contains(':') {
                artists.push(ArtistRef {
                    name: text.to_string(),
                    browse_id: None,
                });
            }
        }
    }

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

/// Parse the library playlists response.
fn parse_library_playlists_response(
    response: &Value,
) -> Result<LibraryResponse<LibraryPlaylist>, ClientError> {
    let mut playlists = Vec::new();

    // Find the grid or shelf containing playlists
    let (contents, cont_token) = if let Some(cont) = response.get("continuationContents") {
        let grid = cont.get("gridContinuation");
        let items = grid.and_then(|g| g.get("items")).and_then(|i| i.as_array());
        let token = grid
            .and_then(|g| g.get("continuations"))
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.pointer("/nextContinuationData/continuation"))
            .and_then(|v| v.as_str())
            .map(String::from);
        (items, token)
    } else {
        // Initial response - playlists are usually in a grid
        let grid = response
            .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/gridRenderer");
        let items = grid.and_then(|g| g.get("items")).and_then(|i| i.as_array());
        let token = grid
            .and_then(|g| g.get("continuations"))
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.pointer("/nextContinuationData/continuation"))
            .and_then(|v| v.as_str())
            .map(String::from);
        (items, token)
    };

    if let Some(items) = contents {
        for item in items {
            if let Some(playlist) = parse_library_playlist(item) {
                playlists.push(playlist);
            }
        }
    }

    Ok(LibraryResponse {
        items: playlists,
        continuation: cont_token,
    })
}

/// Parse a single library playlist.
fn parse_library_playlist(item: &Value) -> Option<LibraryPlaylist> {
    let renderer = item.get("musicTwoRowItemRenderer")?;

    // Get playlist ID from navigation endpoint
    let playlist_id = renderer
        .pointer("/navigationEndpoint/browseEndpoint/browseId")
        .and_then(|v| v.as_str())
        .map(|s| s.strip_prefix("VL").unwrap_or(s).to_string())?;

    // Get title
    let title = renderer
        .pointer("/title/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Get thumbnail
    let thumbnail_url = renderer
        .pointer("/thumbnailRenderer/musicThumbnailRenderer/thumbnail/thumbnails/0/url")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Get track count from subtitle
    let track_count = renderer
        .pointer("/subtitle/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from);

    Some(LibraryPlaylist {
        playlist_id,
        title,
        thumbnail_url,
        track_count,
    })
}

/// Parse the library albums response.
fn parse_library_albums_response(
    response: &Value,
) -> Result<LibraryResponse<LibraryAlbum>, ClientError> {
    let mut albums = Vec::new();

    let (contents, cont_token) = if let Some(cont) = response.get("continuationContents") {
        let grid = cont.get("gridContinuation");
        let items = grid.and_then(|g| g.get("items")).and_then(|i| i.as_array());
        let token = grid
            .and_then(|g| g.get("continuations"))
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.pointer("/nextContinuationData/continuation"))
            .and_then(|v| v.as_str())
            .map(String::from);
        (items, token)
    } else {
        let grid = response
            .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/gridRenderer");
        let items = grid.and_then(|g| g.get("items")).and_then(|i| i.as_array());
        let token = grid
            .and_then(|g| g.get("continuations"))
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.pointer("/nextContinuationData/continuation"))
            .and_then(|v| v.as_str())
            .map(String::from);
        (items, token)
    };

    if let Some(items) = contents {
        for item in items {
            if let Some(album) = parse_library_album(item) {
                albums.push(album);
            }
        }
    }

    Ok(LibraryResponse {
        items: albums,
        continuation: cont_token,
    })
}

/// Parse a single library album.
fn parse_library_album(item: &Value) -> Option<LibraryAlbum> {
    let renderer = item.get("musicTwoRowItemRenderer")?;

    // Get browse ID
    let browse_id = renderer
        .pointer("/navigationEndpoint/browseEndpoint/browseId")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Get title
    let title = renderer
        .pointer("/title/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Get thumbnail
    let thumbnail_url = renderer
        .pointer("/thumbnailRenderer/musicThumbnailRenderer/thumbnail/thumbnails/0/url")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Parse subtitle for artist and year
    let subtitle_runs = renderer
        .pointer("/subtitle/runs")
        .and_then(|v| v.as_array());

    let mut artists = Vec::new();
    let mut year: Option<String> = None;

    if let Some(runs) = subtitle_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");

            // Skip separators and type labels
            if text == " • " || text == " · " || text == "Album" || text == "EP" || text == "Single"
            {
                continue;
            }

            // Check for year
            if text.len() == 4 && text.chars().all(|c| c.is_ascii_digit()) {
                year = Some(text.to_string());
                continue;
            }

            // Check for artist browse endpoint
            let browse_id = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str())
                .map(String::from);

            if !text.is_empty() && !text.starts_with(' ') {
                artists.push(ArtistRef {
                    name: text.to_string(),
                    browse_id,
                });
            }
        }
    }

    // Check for explicit badge
    let is_explicit = renderer
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

    Some(LibraryAlbum {
        browse_id,
        title,
        artists,
        thumbnail_url,
        year,
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

    fn mock_liked_songs_response() -> Value {
        json!({
            "contents": {
                "singleColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicShelfRenderer": {
                                            "contents": [{
                                                "musicResponsiveListItemRenderer": {
                                                    "playlistItemData": {
                                                        "videoId": "dQw4w9WgXcQ",
                                                        "playlistSetVideoId": "abc123setid"
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
                                                                    "runs": [
                                                                        {
                                                                            "text": "Rick Astley",
                                                                            "navigationEndpoint": {
                                                                                "browseEndpoint": {
                                                                                    "browseId": "UCuAXFkgsw1L7xaCfnd5JJOw"
                                                                                }
                                                                            }
                                                                        },
                                                                        {"text": " • "},
                                                                        {
                                                                            "text": "Whenever You Need Somebody",
                                                                            "navigationEndpoint": {
                                                                                "browseEndpoint": {
                                                                                    "browseId": "MPREb_abc123"
                                                                                }
                                                                            }
                                                                        },
                                                                        {"text": " • "},
                                                                        {"text": "3:33"}
                                                                    ]
                                                                }
                                                            }
                                                        }
                                                    ],
                                                    "thumbnail": {
                                                        "musicThumbnailRenderer": {
                                                            "thumbnail": {
                                                                "thumbnails": [{
                                                                    "url": "https://i.ytimg.com/vi/dQw4w9WgXcQ/mqdefault.jpg"
                                                                }]
                                                            }
                                                        }
                                                    }
                                                }
                                            }],
                                            "continuations": [{
                                                "nextContinuationData": {
                                                    "continuation": "next_page_token_123"
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

    fn mock_liked_songs_continuation_response() -> Value {
        json!({
            "continuationContents": {
                "musicShelfContinuation": {
                    "contents": [{
                        "musicResponsiveListItemRenderer": {
                            "playlistItemData": {
                                "videoId": "song2id"
                            },
                            "flexColumns": [
                                {
                                    "musicResponsiveListItemFlexColumnRenderer": {
                                        "text": {
                                            "runs": [{"text": "Second Song"}]
                                        }
                                    }
                                },
                                {
                                    "musicResponsiveListItemFlexColumnRenderer": {
                                        "text": {
                                            "runs": [{"text": "Another Artist"}]
                                        }
                                    }
                                }
                            ]
                        }
                    }]
                }
            }
        })
    }

    fn mock_library_playlists_response() -> Value {
        json!({
            "contents": {
                "singleColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "gridRenderer": {
                                            "items": [{
                                                "musicTwoRowItemRenderer": {
                                                    "navigationEndpoint": {
                                                        "browseEndpoint": {
                                                            "browseId": "VLPLabc123"
                                                        }
                                                    },
                                                    "title": {
                                                        "runs": [{"text": "My Playlist"}]
                                                    },
                                                    "subtitle": {
                                                        "runs": [{"text": "25 songs"}]
                                                    },
                                                    "thumbnailRenderer": {
                                                        "musicThumbnailRenderer": {
                                                            "thumbnail": {
                                                                "thumbnails": [{
                                                                    "url": "https://example.com/playlist_thumb.jpg"
                                                                }]
                                                            }
                                                        }
                                                    }
                                                }
                                            }],
                                            "continuations": [{
                                                "nextContinuationData": {
                                                    "continuation": "playlist_page_2"
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

    fn mock_library_albums_response() -> Value {
        json!({
            "contents": {
                "singleColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "gridRenderer": {
                                            "items": [{
                                                "musicTwoRowItemRenderer": {
                                                    "navigationEndpoint": {
                                                        "browseEndpoint": {
                                                            "browseId": "MPREb_album123"
                                                        }
                                                    },
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
                                                    "thumbnailRenderer": {
                                                        "musicThumbnailRenderer": {
                                                            "thumbnail": {
                                                                "thumbnails": [{
                                                                    "url": "https://example.com/album_thumb.jpg"
                                                                }]
                                                            }
                                                        }
                                                    }
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

    fn mock_empty_library_response() -> Value {
        json!({
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
    fn test_parse_liked_songs() {
        let response = mock_liked_songs_response();
        let result = parse_library_tracks_response(&response).unwrap();

        assert_eq!(result.items.len(), 1);
        let track = &result.items[0];
        assert_eq!(track.video_id, "dQw4w9WgXcQ");
        assert_eq!(track.title, "Never Gonna Give You Up");
        assert_eq!(track.artists.len(), 1);
        assert_eq!(track.artists[0].name, "Rick Astley");
        assert!(track.album.is_some());
        assert_eq!(
            track.album.as_ref().unwrap().title,
            "Whenever You Need Somebody"
        );
        assert_eq!(track.duration_seconds, Some(213));
        assert_eq!(track.set_video_id, Some("abc123setid".to_string()));
        assert!(track.thumbnail_url.is_some());
    }

    #[test]
    fn test_parse_liked_songs_continuation() {
        let response = mock_liked_songs_response();
        let result = parse_library_tracks_response(&response).unwrap();

        assert_eq!(result.continuation, Some("next_page_token_123".to_string()));
    }

    #[test]
    fn test_parse_liked_songs_continuation_response() {
        let response = mock_liked_songs_continuation_response();
        let result = parse_library_tracks_response(&response).unwrap();

        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].video_id, "song2id");
        assert_eq!(result.items[0].title, "Second Song");
    }

    #[test]
    fn test_parse_library_playlists() {
        let response = mock_library_playlists_response();
        let result = parse_library_playlists_response(&response).unwrap();

        assert_eq!(result.items.len(), 1);
        let playlist = &result.items[0];
        assert_eq!(playlist.playlist_id, "PLabc123");
        assert_eq!(playlist.title, "My Playlist");
        assert_eq!(playlist.track_count, Some("25 songs".to_string()));
        assert!(playlist.thumbnail_url.is_some());
    }

    #[test]
    fn test_parse_library_playlists_continuation() {
        let response = mock_library_playlists_response();
        let result = parse_library_playlists_response(&response).unwrap();

        assert_eq!(result.continuation, Some("playlist_page_2".to_string()));
    }

    #[test]
    fn test_parse_library_albums() {
        let response = mock_library_albums_response();
        let result = parse_library_albums_response(&response).unwrap();

        assert_eq!(result.items.len(), 1);
        let album = &result.items[0];
        assert_eq!(album.browse_id, "MPREb_album123");
        assert_eq!(album.title, "Whenever You Need Somebody");
        assert_eq!(album.artists.len(), 1);
        assert_eq!(album.artists[0].name, "Rick Astley");
        assert_eq!(album.year, Some("1987".to_string()));
        assert!(album.thumbnail_url.is_some());
    }

    #[test]
    fn test_parse_empty_library() {
        let response = mock_empty_library_response();

        let tracks_result = parse_library_tracks_response(&response).unwrap();
        assert!(tracks_result.items.is_empty());
        assert!(tracks_result.continuation.is_none());

        let playlists_result = parse_library_playlists_response(&response).unwrap();
        assert!(playlists_result.items.is_empty());

        let albums_result = parse_library_albums_response(&response).unwrap();
        assert!(albums_result.items.is_empty());
    }

    #[test]
    fn test_library_track_serialization() {
        let track = LibraryTrack {
            video_id: "abc123".to_string(),
            title: "Test Song".to_string(),
            artists: vec![ArtistRef {
                name: "Test Artist".to_string(),
                browse_id: Some("UC123".to_string()),
            }],
            album: None,
            duration_seconds: Some(180),
            thumbnail_url: None,
            is_explicit: false,
            set_video_id: Some("set123".to_string()),
        };

        let json = serde_json::to_string(&track).unwrap();
        let deserialized: LibraryTrack = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.video_id, track.video_id);
        assert_eq!(deserialized.set_video_id, track.set_video_id);
    }
}
