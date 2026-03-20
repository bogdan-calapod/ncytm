//! YouTube Music search API.
//!
//! Provides search functionality for tracks, albums, artists, and playlists.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::youtube_music::{ClientError, YouTubeMusicClient};

/// Results from a YouTube Music search.
#[derive(Debug, Clone, Default)]
pub struct SearchResults {
    /// Tracks (songs) matching the search query.
    pub tracks: Vec<SearchTrack>,
    /// Albums matching the search query.
    pub albums: Vec<SearchAlbum>,
    /// Artists matching the search query.
    pub artists: Vec<SearchArtist>,
    /// Playlists matching the search query.
    pub playlists: Vec<SearchPlaylist>,
}

/// A track from search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchTrack {
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

/// An album from search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAlbum {
    /// YouTube browse ID for the album.
    pub browse_id: String,
    /// Album title.
    pub title: String,
    /// Artist names.
    pub artists: Vec<ArtistRef>,
    /// Release year (if available).
    pub year: Option<String>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Whether this is an explicit album.
    pub is_explicit: bool,
}

/// An artist from search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchArtist {
    /// YouTube browse ID for the artist.
    pub browse_id: String,
    /// Artist name.
    pub name: String,
    /// Subscriber count (formatted string, e.g., "1.5M subscribers").
    pub subscribers: Option<String>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
}

/// A playlist from search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPlaylist {
    /// YouTube browse ID for the playlist.
    pub browse_id: String,
    /// Playlist title.
    pub title: String,
    /// Author/creator name.
    pub author: Option<String>,
    /// Track count (formatted string).
    pub track_count: Option<String>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
}

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

/// Perform a search on YouTube Music.
///
/// # Arguments
///
/// * `client` - The YouTube Music API client
/// * `query` - The search query string
///
/// # Returns
///
/// Search results containing tracks, albums, artists, and playlists.
pub async fn search(client: &YouTubeMusicClient, query: &str) -> Result<SearchResults, ClientError> {
    let body = serde_json::json!({
        "query": query,
        "params": "EgWKAQIIAWoKEAMQBBAKEAkQBQ%3D%3D"  // Search all categories
    });

    let response = client.post("search", &body).await?;
    parse_search_response(&response)
}

/// Parse the search API response into SearchResults.
fn parse_search_response(response: &Value) -> Result<SearchResults, ClientError> {
    let mut results = SearchResults::default();

    // Navigate to the shelf contents
    // Response structure: contents.tabbedSearchResultsRenderer.tabs[0].tabRenderer.content.sectionListRenderer.contents
    let contents = response
        .pointer("/contents/tabbedSearchResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .or_else(|| response.pointer("/contents/sectionListRenderer/contents"));

    let Some(contents) = contents.and_then(|v| v.as_array()) else {
        // Empty search results
        return Ok(results);
    };

    for section in contents {
        // Each section is a musicShelfRenderer
        let Some(shelf) = section.get("musicShelfRenderer") else {
            continue;
        };

        // Determine the category from the shelf title
        let category = shelf
            .pointer("/title/runs/0/text")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let Some(items) = shelf.get("contents").and_then(|v| v.as_array()) else {
            continue;
        };

        for item in items {
            let Some(renderer) = item.get("musicResponsiveListItemRenderer") else {
                continue;
            };

            match category.to_lowercase().as_str() {
                "songs" | "top result" => {
                    if let Some(track) = parse_track(renderer) {
                        results.tracks.push(track);
                    }
                }
                "albums" => {
                    if let Some(album) = parse_album(renderer) {
                        results.albums.push(album);
                    }
                }
                "artists" => {
                    if let Some(artist) = parse_artist(renderer) {
                        results.artists.push(artist);
                    }
                }
                "community playlists" | "playlists" | "featured playlists" => {
                    if let Some(playlist) = parse_playlist(renderer) {
                        results.playlists.push(playlist);
                    }
                }
                _ => {
                    // Try to parse as track for "Top result" or unknown categories
                    if let Some(track) = parse_track(renderer) {
                        results.tracks.push(track);
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Parse a track from a musicResponsiveListItemRenderer.
fn parse_track(renderer: &Value) -> Option<SearchTrack> {
    // Get video ID from playlistItemData or overlay
    let video_id = renderer
        .pointer("/playlistItemData/videoId")
        .or_else(|| renderer.pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId"))
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Get flex columns for title, artist, album info
    let flex_columns = renderer.get("flexColumns")?.as_array()?;

    // First column: title
    let title = flex_columns
        .first()?
        .pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Second column: artist - album - duration
    let second_column_runs = flex_columns
        .get(1)?
        .pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs")
        .and_then(|v| v.as_array());

    let mut artists = Vec::new();
    let mut album: Option<AlbumRef> = None;
    let mut duration_seconds: Option<u32> = None;

    if let Some(runs) = second_column_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");
            
            // Skip separators
            if text == " • " || text == " & " || text == ", " || text == " · " {
                continue;
            }

            // Check for navigation endpoint to determine type
            let browse_id = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str())
                .map(String::from);

            // Determine what type this is based on browse_id prefix
            if let Some(ref id) = browse_id {
                if id.starts_with("UC") {
                    // Artist channel ID
                    artists.push(ArtistRef {
                        name: text.to_string(),
                        browse_id: Some(id.clone()),
                    });
                    continue;
                } else if id.starts_with("MPREb") {
                    // Album browse ID
                    album = Some(AlbumRef {
                        title: text.to_string(),
                        browse_id: Some(id.clone()),
                    });
                    continue;
                }
            }

            // Parse duration if it looks like a time (contains :)
            if text.contains(':') {
                duration_seconds = parse_duration(text);
                continue;
            }

            // If no browse_id and not duration, assume it's an artist
            if browse_id.is_none() && !text.contains(':') {
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

    Some(SearchTrack {
        video_id,
        title,
        artists,
        album,
        duration_seconds,
        thumbnail_url,
        is_explicit,
    })
}

/// Parse an album from a musicResponsiveListItemRenderer.
fn parse_album(renderer: &Value) -> Option<SearchAlbum> {
    // Get browse ID from navigation endpoint
    let browse_id = renderer
        .pointer("/navigationEndpoint/browseEndpoint/browseId")
        .or_else(|| renderer.pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchPlaylistEndpoint/playlistId"))
        .and_then(|v| v.as_str())
        .map(String::from)?;

    let flex_columns = renderer.get("flexColumns")?.as_array()?;

    // First column: title
    let title = flex_columns
        .first()?
        .pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Second column: type - artist - year
    let second_column_runs = flex_columns
        .get(1)?
        .pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs")
        .and_then(|v| v.as_array());

    let mut artists = Vec::new();
    let mut year: Option<String> = None;

    if let Some(runs) = second_column_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");
            
            // Skip separators and type labels
            if text == " • " || text == "Album" || text == "EP" || text == "Single" || text == " · " {
                continue;
            }

            // Check for year (4 digits)
            if text.len() == 4 && text.chars().all(|c| c.is_ascii_digit()) {
                year = Some(text.to_string());
                continue;
            }

            // Check for navigation endpoint (artist)
            let browse_id = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str())
                .map(String::from);

            if browse_id.is_some() || (!text.is_empty() && !text.starts_with(' ')) {
                artists.push(ArtistRef {
                    name: text.to_string(),
                    browse_id,
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

    Some(SearchAlbum {
        browse_id,
        title,
        artists,
        year,
        thumbnail_url,
        is_explicit,
    })
}

/// Parse an artist from a musicResponsiveListItemRenderer.
fn parse_artist(renderer: &Value) -> Option<SearchArtist> {
    // Get browse ID from navigation endpoint
    let browse_id = renderer
        .pointer("/navigationEndpoint/browseEndpoint/browseId")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    let flex_columns = renderer.get("flexColumns")?.as_array()?;

    // First column: name
    let name = flex_columns
        .first()?
        .pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Second column: subscribers
    let subscribers = flex_columns
        .get(1)
        .and_then(|col| col.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Get thumbnail
    let thumbnail_url = renderer
        .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails/0/url")
        .and_then(|v| v.as_str())
        .map(String::from);

    Some(SearchArtist {
        browse_id,
        name,
        subscribers,
        thumbnail_url,
    })
}

/// Parse a playlist from a musicResponsiveListItemRenderer.
fn parse_playlist(renderer: &Value) -> Option<SearchPlaylist> {
    // Get browse ID from navigation endpoint
    let browse_id = renderer
        .pointer("/navigationEndpoint/browseEndpoint/browseId")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    let flex_columns = renderer.get("flexColumns")?.as_array()?;

    // First column: title
    let title = flex_columns
        .first()?
        .pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Second column: author - track count
    let second_column_runs = flex_columns
        .get(1)?
        .pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs")
        .and_then(|v| v.as_array());

    let mut author: Option<String> = None;
    let mut track_count: Option<String> = None;

    if let Some(runs) = second_column_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");
            
            // Skip separators
            if text == " • " || text == " · " || text == "Playlist" {
                continue;
            }

            // Check if it mentions tracks/songs
            if text.to_lowercase().contains("song") || text.to_lowercase().contains("track") {
                track_count = Some(text.to_string());
                continue;
            }

            // Otherwise assume it's the author
            if author.is_none() && !text.is_empty() {
                author = Some(text.to_string());
            }
        }
    }

    // Get thumbnail
    let thumbnail_url = renderer
        .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails/0/url")
        .and_then(|v| v.as_str())
        .map(String::from);

    Some(SearchPlaylist {
        browse_id,
        title,
        author,
        track_count,
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
    use serde_json::json;

    /// Create a mock search response with songs
    fn mock_search_response_with_songs() -> Value {
        json!({
            "contents": {
                "tabbedSearchResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicShelfRenderer": {
                                            "title": {
                                                "runs": [{"text": "Songs"}]
                                            },
                                            "contents": [{
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

    /// Create a mock search response with albums
    fn mock_search_response_with_albums() -> Value {
        json!({
            "contents": {
                "tabbedSearchResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicShelfRenderer": {
                                            "title": {
                                                "runs": [{"text": "Albums"}]
                                            },
                                            "contents": [{
                                                "musicResponsiveListItemRenderer": {
                                                    "navigationEndpoint": {
                                                        "browseEndpoint": {
                                                            "browseId": "MPREb_KQMqQyNRSwp"
                                                        }
                                                    },
                                                    "flexColumns": [
                                                        {
                                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                                "text": {
                                                                    "runs": [{"text": "Whenever You Need Somebody"}]
                                                                }
                                                            }
                                                        },
                                                        {
                                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                                "text": {
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
                                                                }
                                                            }
                                                        }
                                                    ],
                                                    "thumbnail": {
                                                        "musicThumbnailRenderer": {
                                                            "thumbnail": {
                                                                "thumbnails": [{
                                                                    "url": "https://lh3.googleusercontent.com/album_thumb"
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

    /// Create a mock search response with artists
    fn mock_search_response_with_artists() -> Value {
        json!({
            "contents": {
                "tabbedSearchResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicShelfRenderer": {
                                            "title": {
                                                "runs": [{"text": "Artists"}]
                                            },
                                            "contents": [{
                                                "musicResponsiveListItemRenderer": {
                                                    "navigationEndpoint": {
                                                        "browseEndpoint": {
                                                            "browseId": "UCuAXFkgsw1L7xaCfnd5JJOw"
                                                        }
                                                    },
                                                    "flexColumns": [
                                                        {
                                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                                "text": {
                                                                    "runs": [{"text": "Rick Astley"}]
                                                                }
                                                            }
                                                        },
                                                        {
                                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                                "text": {
                                                                    "runs": [{"text": "2.5M subscribers"}]
                                                                }
                                                            }
                                                        }
                                                    ],
                                                    "thumbnail": {
                                                        "musicThumbnailRenderer": {
                                                            "thumbnail": {
                                                                "thumbnails": [{
                                                                    "url": "https://lh3.googleusercontent.com/artist_thumb"
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

    /// Create a mock search response with playlists
    fn mock_search_response_with_playlists() -> Value {
        json!({
            "contents": {
                "tabbedSearchResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicShelfRenderer": {
                                            "title": {
                                                "runs": [{"text": "Community playlists"}]
                                            },
                                            "contents": [{
                                                "musicResponsiveListItemRenderer": {
                                                    "navigationEndpoint": {
                                                        "browseEndpoint": {
                                                            "browseId": "VLPL_xyz123"
                                                        }
                                                    },
                                                    "flexColumns": [
                                                        {
                                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                                "text": {
                                                                    "runs": [{"text": "80s Hits"}]
                                                                }
                                                            }
                                                        },
                                                        {
                                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                                "text": {
                                                                    "runs": [
                                                                        {"text": "Playlist"},
                                                                        {"text": " • "},
                                                                        {"text": "Music Fan"},
                                                                        {"text": " • "},
                                                                        {"text": "150 songs"}
                                                                    ]
                                                                }
                                                            }
                                                        }
                                                    ],
                                                    "thumbnail": {
                                                        "musicThumbnailRenderer": {
                                                            "thumbnail": {
                                                                "thumbnails": [{
                                                                    "url": "https://lh3.googleusercontent.com/playlist_thumb"
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

    /// Create an empty search response
    fn mock_empty_search_response() -> Value {
        json!({
            "contents": {
                "tabbedSearchResultsRenderer": {
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
    fn test_parse_search_response_with_songs() {
        let response = mock_search_response_with_songs();
        let results = parse_search_response(&response).unwrap();

        assert_eq!(results.tracks.len(), 1);
        let track = &results.tracks[0];
        assert_eq!(track.video_id, "dQw4w9WgXcQ");
        assert_eq!(track.title, "Never Gonna Give You Up");
        assert_eq!(track.artists.len(), 1);
        assert_eq!(track.artists[0].name, "Rick Astley");
        assert_eq!(track.artists[0].browse_id, Some("UCuAXFkgsw1L7xaCfnd5JJOw".to_string()));
        assert!(track.album.is_some());
        let album = track.album.as_ref().unwrap();
        assert_eq!(album.title, "Whenever You Need Somebody");
        assert_eq!(album.browse_id, Some("MPREb_abc123".to_string()));
        assert_eq!(track.duration_seconds, Some(213)); // 3:33 = 213 seconds
        assert!(track.thumbnail_url.is_some());
    }

    #[test]
    fn test_parse_search_response_with_albums() {
        let response = mock_search_response_with_albums();
        let results = parse_search_response(&response).unwrap();

        assert_eq!(results.albums.len(), 1);
        let album = &results.albums[0];
        assert_eq!(album.browse_id, "MPREb_KQMqQyNRSwp");
        assert_eq!(album.title, "Whenever You Need Somebody");
        assert_eq!(album.artists.len(), 1);
        assert_eq!(album.artists[0].name, "Rick Astley");
        assert_eq!(album.year, Some("1987".to_string()));
        assert!(album.thumbnail_url.is_some());
    }

    #[test]
    fn test_parse_search_response_with_artists() {
        let response = mock_search_response_with_artists();
        let results = parse_search_response(&response).unwrap();

        assert_eq!(results.artists.len(), 1);
        let artist = &results.artists[0];
        assert_eq!(artist.browse_id, "UCuAXFkgsw1L7xaCfnd5JJOw");
        assert_eq!(artist.name, "Rick Astley");
        assert_eq!(artist.subscribers, Some("2.5M subscribers".to_string()));
        assert!(artist.thumbnail_url.is_some());
    }

    #[test]
    fn test_parse_search_response_with_playlists() {
        let response = mock_search_response_with_playlists();
        let results = parse_search_response(&response).unwrap();

        assert_eq!(results.playlists.len(), 1);
        let playlist = &results.playlists[0];
        assert_eq!(playlist.browse_id, "VLPL_xyz123");
        assert_eq!(playlist.title, "80s Hits");
        assert_eq!(playlist.author, Some("Music Fan".to_string()));
        assert_eq!(playlist.track_count, Some("150 songs".to_string()));
        assert!(playlist.thumbnail_url.is_some());
    }

    #[test]
    fn test_parse_empty_search_response() {
        let response = mock_empty_search_response();
        let results = parse_search_response(&response).unwrap();

        assert!(results.tracks.is_empty());
        assert!(results.albums.is_empty());
        assert!(results.artists.is_empty());
        assert!(results.playlists.is_empty());
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("3:33"), Some(213));
        assert_eq!(parse_duration("0:30"), Some(30));
        assert_eq!(parse_duration("10:00"), Some(600));
        assert_eq!(parse_duration("1:00:00"), Some(3600));
        assert_eq!(parse_duration("1:30:45"), Some(5445));
        assert_eq!(parse_duration("invalid"), None);
        assert_eq!(parse_duration(""), None);
    }

    #[test]
    fn test_search_track_serialization() {
        let track = SearchTrack {
            video_id: "abc123".to_string(),
            title: "Test Song".to_string(),
            artists: vec![ArtistRef {
                name: "Test Artist".to_string(),
                browse_id: Some("UC123".to_string()),
            }],
            album: Some(AlbumRef {
                title: "Test Album".to_string(),
                browse_id: Some("MPREb_123".to_string()),
            }),
            duration_seconds: Some(180),
            thumbnail_url: Some("https://example.com/thumb.jpg".to_string()),
            is_explicit: false,
        };

        let json = serde_json::to_string(&track).unwrap();
        let deserialized: SearchTrack = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.video_id, track.video_id);
        assert_eq!(deserialized.title, track.title);
        assert_eq!(deserialized.artists.len(), 1);
    }

    #[test]
    fn test_parse_track_with_explicit_badge() {
        let response = json!({
            "contents": {
                "tabbedSearchResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicShelfRenderer": {
                                            "title": {
                                                "runs": [{"text": "Songs"}]
                                            },
                                            "contents": [{
                                                "musicResponsiveListItemRenderer": {
                                                    "playlistItemData": {
                                                        "videoId": "explicit123"
                                                    },
                                                    "flexColumns": [
                                                        {
                                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                                "text": {
                                                                    "runs": [{"text": "Explicit Song"}]
                                                                }
                                                            }
                                                        },
                                                        {
                                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                                "text": {
                                                                    "runs": [
                                                                        {"text": "Some Artist"}
                                                                    ]
                                                                }
                                                            }
                                                        }
                                                    ],
                                                    "badges": [{
                                                        "musicInlineBadgeRenderer": {
                                                            "icon": {
                                                                "iconType": "MUSIC_EXPLICIT_BADGE"
                                                            }
                                                        }
                                                    }]
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
        });

        let results = parse_search_response(&response).unwrap();
        assert_eq!(results.tracks.len(), 1);
        assert!(results.tracks[0].is_explicit);
    }

    #[test]
    fn test_parse_malformed_response_returns_empty() {
        let response = json!({
            "error": "something went wrong"
        });

        let results = parse_search_response(&response).unwrap();
        assert!(results.tracks.is_empty());
        assert!(results.albums.is_empty());
        assert!(results.artists.is_empty());
        assert!(results.playlists.is_empty());
    }
}
