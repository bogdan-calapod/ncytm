//! YouTube Music playlist API.
//!
//! Provides CRUD operations for playlists: fetching tracks, creating, deleting,
//! adding/removing tracks, and following/unfollowing playlists.

use serde_json::{Value, json};

use super::library::ArtistRef;
use crate::youtube_music::{ClientError, YouTubeMusicClient};

/// A track inside a playlist.
#[derive(Debug, Clone)]
pub struct PlaylistTrack {
    /// YouTube video ID.
    pub video_id: String,
    /// Track title.
    pub title: String,
    /// Artists on this track.
    pub artists: Vec<ArtistRef>,
    /// Album title (if available).
    pub album: Option<String>,
    /// Album browse ID (if available).
    pub album_id: Option<String>,
    /// Duration in seconds.
    pub duration_seconds: Option<u32>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Whether this is explicit.
    pub is_explicit: bool,
    /// Set video ID — required for removing this track from the playlist.
    pub set_video_id: Option<String>,
}

/// Playlist metadata returned from a browse request.
#[derive(Debug, Clone)]
pub struct PlaylistInfo {
    /// Playlist ID.
    pub playlist_id: String,
    /// Playlist title.
    pub title: String,
    /// Owner channel ID.
    pub owner_id: String,
    /// Owner display name.
    pub owner_name: Option<String>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Total number of tracks.
    pub track_count: usize,
}

/// Response from playlist track fetch, with optional continuation.
#[derive(Debug, Clone)]
pub struct PlaylistTracksResponse {
    #[allow(dead_code)]
    pub info: Option<PlaylistInfo>,
    pub tracks: Vec<PlaylistTrack>,
    pub continuation: Option<String>,
}

// ── Read operations ──────────────────────────────────────────────────────────

/// Fetch tracks from a playlist.
///
/// The `playlist_id` should be the raw playlist ID (e.g. `PLxxx`).
/// Pass `continuation` for subsequent pages.
pub async fn get_playlist_tracks(
    client: &YouTubeMusicClient,
    playlist_id: &str,
    continuation: Option<&str>,
) -> Result<PlaylistTracksResponse, ClientError> {
    let response = if let Some(token) = continuation {
        let body = json!({ "continuation": token });
        client.post("browse", &body).await?
    } else {
        // YouTube Music uses "VL" + playlist_id as the browse ID for playlists
        let browse_id = if playlist_id.starts_with("VL") {
            playlist_id.to_string()
        } else {
            format!("VL{playlist_id}")
        };
        let body = json!({ "browseId": browse_id });
        client.post("browse", &body).await?
    };

    Ok(parse_playlist_tracks_response(&response, playlist_id))
}

/// Fetch playlist metadata (title, owner, etc.) by playlist ID.
pub async fn get_playlist_info(
    client: &YouTubeMusicClient,
    playlist_id: &str,
) -> Result<Option<PlaylistInfo>, ClientError> {
    let browse_id = if playlist_id.starts_with("VL") {
        playlist_id.to_string()
    } else {
        format!("VL{playlist_id}")
    };
    let body = json!({ "browseId": browse_id });
    let response = client.post("browse", &body).await?;
    Ok(parse_playlist_info(&response, playlist_id))
}

// ── Write operations ──────────────────────────────────────────────────────────

/// Create a new playlist.
///
/// Returns the new playlist ID on success.
pub async fn create_playlist(
    client: &YouTubeMusicClient,
    title: &str,
    description: Option<&str>,
    public: bool,
) -> Result<String, ClientError> {
    let privacy = if public { "PUBLIC" } else { "PRIVATE" };
    let body = json!({
        "title": title,
        "description": description.unwrap_or(""),
        "privacyStatus": privacy,
        "videoIds": []
    });

    let response = client.post("playlist/create", &body).await?;

    // The response contains the new playlist ID
    let playlist_id = response
        .get("playlistId")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| ClientError::ApiError {
            message: "No playlistId in create_playlist response".to_string(),
        })?;

    Ok(playlist_id)
}

/// Delete a playlist by ID.
pub async fn delete_playlist(
    client: &YouTubeMusicClient,
    playlist_id: &str,
) -> Result<(), ClientError> {
    let body = json!({ "playlistId": playlist_id });
    client.post("playlist/delete", &body).await?;
    Ok(())
}

/// Add video IDs to a playlist.
///
/// Returns the set_video_ids assigned to the newly added tracks (needed for later removal).
pub async fn add_playlist_tracks(
    client: &YouTubeMusicClient,
    playlist_id: &str,
    video_ids: &[&str],
) -> Result<Vec<String>, ClientError> {
    let actions: Vec<Value> = video_ids
        .iter()
        .map(|id| {
            json!({
                "action": "ACTION_ADD_VIDEO",
                "addedVideoId": id
            })
        })
        .collect();

    let body = json!({
        "playlistId": playlist_id,
        "actions": actions
    });

    let response = client.post("browse/edit_playlist", &body).await?;

    // Extract set_video_ids from the response (needed to delete the tracks later)
    let set_video_ids = response
        .pointer("/playlistEditResults")
        .and_then(|v| v.as_array())
        .map(|results| {
            results
                .iter()
                .filter_map(|r| {
                    r.pointer("/playlistEditVideoAddedResultData/setVideoId")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(set_video_ids)
}

/// Remove tracks from a playlist by their set_video_id.
///
/// The `set_video_id` is the per-playlist instance ID (different from the video_id).
/// It is returned when adding tracks, and also present in playlist track listings.
pub async fn remove_playlist_tracks(
    client: &YouTubeMusicClient,
    playlist_id: &str,
    set_video_ids: &[&str],
    video_ids: &[&str],
) -> Result<(), ClientError> {
    let actions: Vec<Value> = set_video_ids
        .iter()
        .zip(video_ids.iter())
        .map(|(svid, vid)| {
            json!({
                "action": "ACTION_REMOVE_VIDEO",
                "removedVideoId": vid,
                "setVideoId": svid
            })
        })
        .collect();

    let body = json!({
        "playlistId": playlist_id,
        "actions": actions
    });

    client.post("browse/edit_playlist", &body).await?;
    Ok(())
}

/// Follow (save to library) a playlist.
///
/// YouTube Music saves a playlist by adding it to the user's library via the
/// `playlist/edit` endpoint with an ACTION_ADD_PLAYLIST_TO_LIBRARY action,
/// or simply by browsing it (which auto-saves). We use the like endpoint.
pub async fn follow_playlist(
    client: &YouTubeMusicClient,
    playlist_id: &str,
) -> Result<(), ClientError> {
    // Save/follow by adding to library
    let body = json!({
        "playlistId": playlist_id
    });
    client.post("playlist/edit", &body).await?;
    Ok(())
}

/// Unfollow (remove from library) a playlist.
pub async fn unfollow_playlist(
    client: &YouTubeMusicClient,
    playlist_id: &str,
) -> Result<(), ClientError> {
    let body = json!({
        "playlistId": playlist_id,
        "actions": [{ "action": "ACTION_REMOVE_PLAYLIST_FROM_LIBRARY" }]
    });
    client.post("browse/edit_playlist", &body).await?;
    Ok(())
}

// ── Parsers ───────────────────────────────────────────────────────────────────

fn parse_playlist_info(response: &Value, playlist_id: &str) -> Option<PlaylistInfo> {
    // Header can be musicDetailHeaderRenderer or musicEditablePlaylistDetailHeaderRenderer
    let header = response
        .pointer("/header/musicDetailHeaderRenderer")
        .or_else(|| {
            response.pointer(
                "/header/musicEditablePlaylistDetailHeaderRenderer/header/musicDetailHeaderRenderer",
            )
        })?;

    let title = header
        .pointer("/title/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    let thumbnail_url = header
        .pointer("/thumbnail/croppedSquareThumbnailRenderer/thumbnail/thumbnails")
        .or_else(|| header.pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails"))
        .and_then(|v| v.as_array())
        .and_then(|t| t.last())
        .and_then(|t| t.get("url"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let description = header
        .pointer("/description/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Owner name + id from subtitle
    let subtitle_runs = header.pointer("/subtitle/runs").and_then(|v| v.as_array());

    let mut owner_name: Option<String> = None;
    let mut owner_id = String::new();

    if let Some(runs) = subtitle_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if text == " • " || text.is_empty() {
                continue;
            }
            let bid = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str());
            if let Some(id) = bid {
                owner_id = id.to_string();
                owner_name = Some(text.to_string());
                break;
            }
        }
    }

    Some(PlaylistInfo {
        playlist_id: playlist_id.to_string(),
        title,
        owner_id,
        owner_name,
        thumbnail_url,
        description,
        track_count: 0, // will be filled in after parsing tracks
    })
}

fn parse_playlist_tracks_response(response: &Value, playlist_id: &str) -> PlaylistTracksResponse {
    let info = parse_playlist_info(response, playlist_id);

    let (tracks, continuation) = if let Some(cont) = response.get("continuationContents") {
        // Continuation response
        let shelf = cont.get("musicPlaylistShelfContinuation");
        let items = shelf
            .and_then(|s| s.get("contents"))
            .and_then(|c| c.as_array());
        let token = shelf
            .and_then(|s| s.get("continuations"))
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|c| c.pointer("/nextContinuationData/continuation"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let tracks = items
            .map(|items| items.iter().filter_map(parse_playlist_track).collect())
            .unwrap_or_default();
        (tracks, token)
    } else {
        // Initial response: look for musicShelfRenderer or musicPlaylistShelfRenderer
        let shelf = response
            .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/musicPlaylistShelfRenderer")
            .or_else(|| response.pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/musicShelfRenderer"));

        let items = shelf
            .and_then(|s| s.get("contents"))
            .and_then(|c| c.as_array());

        let token = shelf
            .and_then(|s| s.get("continuations"))
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|c| c.pointer("/nextContinuationData/continuation"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let tracks = items
            .map(|items| items.iter().filter_map(parse_playlist_track).collect())
            .unwrap_or_default();

        (tracks, token)
    };

    PlaylistTracksResponse {
        info,
        tracks,
        continuation,
    }
}

fn parse_playlist_track(item: &Value) -> Option<PlaylistTrack> {
    let renderer = item.get("musicResponsiveListItemRenderer")?;

    // Video ID
    let video_id = renderer
        .pointer("/playlistItemData/videoId")
        .or_else(|| {
            renderer.pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId")
        })
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // set_video_id — used for removal
    let set_video_id = renderer
        .pointer("/playlistItemData/playlistSetVideoId")
        .and_then(|v| v.as_str())
        .map(String::from);

    let flex_columns = renderer.get("flexColumns")?.as_array()?;

    // Title
    let title = flex_columns
        .first()?
        .pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Second column: artists + album
    let second_column_runs = flex_columns
        .get(1)
        .and_then(|col| col.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
        .and_then(|v| v.as_array());

    let mut artists = Vec::new();
    let mut album: Option<String> = None;
    let mut album_id: Option<String> = None;

    if let Some(runs) = second_column_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if text == " • " || text == " & " || text == ", " || text == " · " || text.is_empty()
            {
                continue;
            }
            let browse_id = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str())
                .map(String::from);

            if let Some(ref id) = browse_id
                && id.starts_with("MPREb")
            {
                album = Some(text.to_string());
                album_id = Some(id.clone());
                continue;
            }
            // Artist (UC prefix or no browse ID)
            if browse_id.as_ref().is_none_or(|id| id.starts_with("UC")) && !text.contains(':') {
                artists.push(ArtistRef {
                    name: text.to_string(),
                    browse_id,
                });
            }
        }
    }

    // Duration from fixedColumns
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

    // Thumbnail
    let thumbnail_url = renderer
        .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails/0/url")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Explicit badge
    let is_explicit = renderer
        .get("badges")
        .and_then(|v| v.as_array())
        .is_some_and(|badges| {
            badges.iter().any(|b| {
                b.pointer("/musicInlineBadgeRenderer/icon/iconType")
                    .and_then(|v| v.as_str())
                    == Some("MUSIC_EXPLICIT_BADGE")
            })
        });

    Some(PlaylistTrack {
        video_id,
        title,
        artists,
        album,
        album_id,
        duration_seconds,
        thumbnail_url,
        is_explicit,
        set_video_id,
    })
}

fn parse_duration(s: &str) -> Option<u32> {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        2 => {
            let m: u32 = parts[0].parse().ok()?;
            let s: u32 = parts[1].parse().ok()?;
            Some(m * 60 + s)
        }
        3 => {
            let h: u32 = parts[0].parse().ok()?;
            let m: u32 = parts[1].parse().ok()?;
            let s: u32 = parts[2].parse().ok()?;
            Some(h * 3600 + m * 60 + s)
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
                    "title": { "runs": [{"text": "My Playlist"}] },
                    "subtitle": {
                        "runs": [
                            {"text": "Playlist"},
                            {"text": " • "},
                            {
                                "text": "Test User",
                                "navigationEndpoint": {
                                    "browseEndpoint": { "browseId": "UCowner123" }
                                }
                            }
                        ]
                    },
                    "thumbnail": {
                        "croppedSquareThumbnailRenderer": {
                            "thumbnail": {
                                "thumbnails": [{"url": "https://example.com/thumb.jpg"}]
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
                                            "contents": [
                                                {
                                                    "musicResponsiveListItemRenderer": {
                                                        "playlistItemData": {
                                                            "videoId": "vid1",
                                                            "playlistSetVideoId": "svid1"
                                                        },
                                                        "flexColumns": [
                                                            {
                                                                "musicResponsiveListItemFlexColumnRenderer": {
                                                                    "text": { "runs": [{"text": "Song One"}] }
                                                                }
                                                            },
                                                            {
                                                                "musicResponsiveListItemFlexColumnRenderer": {
                                                                    "text": {
                                                                        "runs": [
                                                                            {
                                                                                "text": "Artist A",
                                                                                "navigationEndpoint": {
                                                                                    "browseEndpoint": {"browseId": "UCa"}
                                                                                }
                                                                            },
                                                                            {"text": " • "},
                                                                            {
                                                                                "text": "The Album",
                                                                                "navigationEndpoint": {
                                                                                    "browseEndpoint": {"browseId": "MPREb_abc"}
                                                                                }
                                                                            }
                                                                        ]
                                                                    }
                                                                }
                                                            }
                                                        ],
                                                        "fixedColumns": [{
                                                            "musicResponsiveListItemFixedColumnRenderer": {
                                                                "text": { "runs": [{"text": "3:45"}] }
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

    #[test]
    fn test_parse_playlist_info() {
        let response = mock_playlist_response();
        let info = parse_playlist_info(&response, "PL123").unwrap();
        assert_eq!(info.title, "My Playlist");
        assert_eq!(info.playlist_id, "PL123");
        assert_eq!(info.owner_name, Some("Test User".to_string()));
        assert_eq!(info.owner_id, "UCowner123");
        assert!(info.thumbnail_url.is_some());
    }

    #[test]
    fn test_parse_playlist_tracks_response() {
        let response = mock_playlist_response();
        let result = parse_playlist_tracks_response(&response, "PL123");
        assert!(result.info.is_some());
        assert_eq!(result.tracks.len(), 1);
        let t = &result.tracks[0];
        assert_eq!(t.video_id, "vid1");
        assert_eq!(t.title, "Song One");
        assert_eq!(t.set_video_id, Some("svid1".to_string()));
        assert_eq!(t.duration_seconds, Some(225));
        assert_eq!(t.artists.len(), 1);
        assert_eq!(t.artists[0].name, "Artist A");
        assert_eq!(t.album, Some("The Album".to_string()));
        assert_eq!(t.album_id, Some("MPREb_abc".to_string()));
        assert!(result.continuation.is_none());
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("3:45"), Some(225));
        assert_eq!(parse_duration("1:00:00"), Some(3600));
        assert_eq!(parse_duration("bad"), None);
    }
}
