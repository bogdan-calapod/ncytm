//! YouTube Music search API.
//!
//! Provides functionality to search for tracks, albums, artists, and playlists.

use serde_json::{Value, json};

use super::library::{AlbumRef, ArtistRef};
use crate::youtube_music::{ClientError, YouTubeMusicClient};

/// A track from search results.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct SearchAlbum {
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
    /// Album type (e.g., "Album", "EP", "Single").
    #[allow(dead_code)]
    pub album_type: Option<String>,
}

/// An artist from search results.
#[derive(Debug, Clone)]
pub struct SearchArtist {
    /// Artist browse ID.
    pub browse_id: String,
    /// Artist name.
    pub name: String,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Subscriber count string (e.g., "1.5M subscribers").
    pub subscribers: Option<String>,
}

/// A playlist from search results.
#[derive(Debug, Clone)]
pub struct SearchPlaylist {
    /// Playlist browse ID.
    pub browse_id: String,
    /// Playlist title.
    pub title: String,
    /// Author name.
    pub author: Option<String>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Track count string (e.g., "50 songs").
    #[allow(dead_code)]
    pub track_count: Option<String>,
}

/// Combined search results.
#[derive(Debug, Clone, Default)]
pub struct SearchResults {
    /// Tracks found.
    pub tracks: Vec<SearchTrack>,
    /// Albums found.
    pub albums: Vec<SearchAlbum>,
    /// Artists found.
    pub artists: Vec<SearchArtist>,
    /// Playlists found.
    pub playlists: Vec<SearchPlaylist>,
}

/// Search filter parameter values for YouTube Music.
const FILTER_SONGS: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA%3D%3D";
const FILTER_ALBUMS: &str = "EgWKAQIYAWoKEAkQBRAKEAMQBA%3D%3D";
const FILTER_ARTISTS: &str = "EgWKAQIgAWoKEAkQBRAKEAMQBA%3D%3D";
const FILTER_PLAYLISTS: &str = "EgeKAQQoAEABagoQCRAFEAoQAxAE";

/// Search YouTube Music for tracks, albums, artists, and playlists.
///
/// # Arguments
///
/// * `client` - The YouTube Music API client
/// * `query` - The search query
///
/// # Returns
///
/// Combined search results containing tracks, albums, artists, and playlists.
pub async fn search(
    client: &YouTubeMusicClient,
    query: &str,
) -> Result<SearchResults, ClientError> {
    // Run all searches in parallel
    let (tracks_result, albums_result, artists_result, playlists_result) = tokio::join!(
        search_tracks(client, query),
        search_albums(client, query),
        search_artists(client, query),
        search_playlists(client, query),
    );

    Ok(SearchResults {
        tracks: tracks_result.unwrap_or_default(),
        albums: albums_result.unwrap_or_default(),
        artists: artists_result.unwrap_or_default(),
        playlists: playlists_result.unwrap_or_default(),
    })
}

/// Search for tracks only.
pub async fn search_tracks(
    client: &YouTubeMusicClient,
    query: &str,
) -> Result<Vec<SearchTrack>, ClientError> {
    let body = json!({
        "query": query,
        "params": FILTER_SONGS
    });

    let response = client.post("search", &body).await?;
    Ok(parse_track_results(&response))
}

/// Search for albums only.
pub async fn search_albums(
    client: &YouTubeMusicClient,
    query: &str,
) -> Result<Vec<SearchAlbum>, ClientError> {
    let body = json!({
        "query": query,
        "params": FILTER_ALBUMS
    });

    let response = client.post("search", &body).await?;
    Ok(parse_album_results(&response))
}

/// Search for artists only.
pub async fn search_artists(
    client: &YouTubeMusicClient,
    query: &str,
) -> Result<Vec<SearchArtist>, ClientError> {
    let body = json!({
        "query": query,
        "params": FILTER_ARTISTS
    });

    let response = client.post("search", &body).await?;
    Ok(parse_artist_results(&response))
}

/// Search for playlists only.
pub async fn search_playlists(
    client: &YouTubeMusicClient,
    query: &str,
) -> Result<Vec<SearchPlaylist>, ClientError> {
    let body = json!({
        "query": query,
        "params": FILTER_PLAYLISTS
    });

    let response = client.post("search", &body).await?;
    Ok(parse_playlist_results(&response))
}

/// Parse track search results.
fn parse_track_results(response: &Value) -> Vec<SearchTrack> {
    let mut tracks = Vec::new();

    // Navigate to the search results contents
    let contents = get_search_contents(response);

    if let Some(contents) = contents {
        for item in contents {
            if let Some(track) = parse_search_track(item) {
                tracks.push(track);
            }
        }
    }

    tracks
}

/// Parse album search results.
fn parse_album_results(response: &Value) -> Vec<SearchAlbum> {
    let mut albums = Vec::new();

    let contents = get_search_contents(response);

    if let Some(contents) = contents {
        for item in contents {
            if let Some(album) = parse_search_album(item) {
                albums.push(album);
            }
        }
    }

    albums
}

/// Parse artist search results.
fn parse_artist_results(response: &Value) -> Vec<SearchArtist> {
    let mut artists = Vec::new();

    let contents = get_search_contents(response);

    if let Some(contents) = contents {
        for item in contents {
            if let Some(artist) = parse_search_artist(item) {
                artists.push(artist);
            }
        }
    }

    artists
}

/// Parse playlist search results.
fn parse_playlist_results(response: &Value) -> Vec<SearchPlaylist> {
    let mut playlists = Vec::new();

    let contents = get_search_contents(response);

    if let Some(contents) = contents {
        for item in contents {
            if let Some(playlist) = parse_search_playlist(item) {
                playlists.push(playlist);
            }
        }
    }

    playlists
}

/// Get search results contents from response.
fn get_search_contents(response: &Value) -> Option<&Vec<Value>> {
    // Path: contents.tabbedSearchResultsRenderer.tabs[0].tabRenderer.content.sectionListRenderer.contents
    response
        .pointer("/contents/tabbedSearchResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|v| v.as_array())
        .and_then(|sections| {
            // Find the section with musicShelfRenderer containing results
            for section in sections {
                if let Some(shelf) = section.get("musicShelfRenderer")
                    && let Some(contents) = shelf.get("contents").and_then(|c| c.as_array())
                {
                    return Some(contents);
                }
            }
            None
        })
}

/// Parse a single track from search results.
fn parse_search_track(item: &Value) -> Option<SearchTrack> {
    let renderer = item.get("musicResponsiveListItemRenderer")?;

    // Get video ID from overlay or flexColumns
    let video_id = renderer
        .pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId")
        .or_else(|| renderer.pointer("/playlistItemData/videoId"))
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

    // Second column: artist - album - duration
    let second_column_runs = flex_columns
        .get(1)
        .and_then(|col| col.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
        .and_then(|v| v.as_array());

    let mut artists = Vec::new();
    let mut album: Option<AlbumRef> = None;

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
                } else if id.starts_with("MPREb") {
                    album = Some(AlbumRef {
                        title: text.to_string(),
                        browse_id: Some(id.clone()),
                    });
                }
            } else if !text.is_empty() && !text.contains(':') {
                // Assume artist if no browse_id and not a duration
                artists.push(ArtistRef {
                    name: text.to_string(),
                    browse_id: None,
                });
            }
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

/// Parse a single album from search results.
fn parse_search_album(item: &Value) -> Option<SearchAlbum> {
    let renderer = item.get("musicResponsiveListItemRenderer")?;

    // Get browse ID
    let browse_id = renderer
        .pointer("/navigationEndpoint/browseEndpoint/browseId")
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

    // Second column: type - artist - year
    let second_column_runs = flex_columns
        .get(1)
        .and_then(|col| col.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
        .and_then(|v| v.as_array());

    let mut artists = Vec::new();
    let mut year: Option<String> = None;
    let mut album_type: Option<String> = None;

    if let Some(runs) = second_column_runs {
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

            let browse_id = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str())
                .map(String::from);

            if !text.is_empty() {
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
        .or_else(|| renderer.get("subtitleBadges"))
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
        thumbnail_url,
        year,
        is_explicit,
        album_type,
    })
}

/// Parse a single artist from search results.
fn parse_search_artist(item: &Value) -> Option<SearchArtist> {
    let renderer = item.get("musicResponsiveListItemRenderer")?;

    // Get browse ID
    let browse_id = renderer
        .pointer("/navigationEndpoint/browseEndpoint/browseId")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Artists should have UC prefix
    if !browse_id.starts_with("UC") {
        return None;
    }

    // Get flex columns
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
        thumbnail_url,
        subscribers,
    })
}

/// Parse a single playlist from search results.
fn parse_search_playlist(item: &Value) -> Option<SearchPlaylist> {
    let renderer = item.get("musicResponsiveListItemRenderer")?;

    // Get browse ID
    let browse_id = renderer
        .pointer("/navigationEndpoint/browseEndpoint/browseId")
        .and_then(|v| v.as_str())
        .map(|s| s.strip_prefix("VL").unwrap_or(s).to_string())?;

    // Get flex columns
    let flex_columns = renderer.get("flexColumns")?.as_array()?;

    // First column: title
    let title = flex_columns
        .first()?
        .pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Second column: author - track count
    let second_column_runs = flex_columns
        .get(1)
        .and_then(|col| col.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
        .and_then(|v| v.as_array());

    let mut author: Option<String> = None;
    let mut track_count: Option<String> = None;

    if let Some(runs) = second_column_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");

            // Skip separators and type labels
            if text == " • " || text == " · " || text == "Playlist" {
                continue;
            }

            // Check if it's a track count
            if text.contains("song") || text.contains("track") {
                track_count = Some(text.to_string());
                continue;
            }

            // Otherwise assume author
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
        thumbnail_url,
        track_count,
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

    fn mock_track_search_response() -> Value {
        json!({
            "contents": {
                "tabbedSearchResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicShelfRenderer": {
                                            "contents": [{
                                                "musicResponsiveListItemRenderer": {
                                                    "overlay": {
                                                        "musicItemThumbnailOverlayRenderer": {
                                                            "content": {
                                                                "musicPlayButtonRenderer": {
                                                                    "playNavigationEndpoint": {
                                                                        "watchEndpoint": {
                                                                            "videoId": "dQw4w9WgXcQ"
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
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
                                                                        }
                                                                    ]
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
                                                    }],
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

    #[test]
    fn test_parse_track_results() {
        let response = mock_track_search_response();
        let tracks = parse_track_results(&response);

        assert_eq!(tracks.len(), 1);
        let track = &tracks[0];
        assert_eq!(track.video_id, "dQw4w9WgXcQ");
        assert_eq!(track.title, "Never Gonna Give You Up");
        assert_eq!(track.artists.len(), 1);
        assert_eq!(track.artists[0].name, "Rick Astley");
        assert_eq!(track.duration_seconds, Some(213)); // 3:33 = 213 seconds
        assert!(track.thumbnail_url.is_some());
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("3:45"), Some(225));
        assert_eq!(parse_duration("0:30"), Some(30));
        assert_eq!(parse_duration("1:00:00"), Some(3600));
        assert_eq!(parse_duration("invalid"), None);
    }

    #[test]
    fn test_empty_response() {
        let empty_response = json!({});
        let tracks = parse_track_results(&empty_response);
        assert!(tracks.is_empty());
    }
}
