//! YouTube Music artist API.
//!
//! Provides access to artist pages: top tracks, albums, singles, and related artists.

use serde_json::{Value, json};

use super::library::{AlbumRef, ArtistRef};
use crate::youtube_music::{ClientError, YouTubeMusicClient};

/// An artist's details from their page.
#[derive(Debug, Clone)]
pub struct ArtistDetails {
    /// Artist browse ID.
    pub browse_id: String,
    /// Artist name.
    pub name: String,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Subscriber count string (e.g., "1.5M subscribers").
    pub subscribers: Option<String>,
    /// Description/bio.
    #[allow(dead_code)]
    pub description: Option<String>,
}

/// A track from an artist's top songs.
#[derive(Debug, Clone)]
pub struct ArtistTrack {
    /// YouTube video ID.
    pub video_id: String,
    /// Track title.
    pub title: String,
    /// Artists on this track.
    pub artists: Vec<ArtistRef>,
    /// Album reference (if available).
    pub album: Option<AlbumRef>,
    /// Duration in seconds.
    pub duration_seconds: Option<u32>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Whether this is an explicit track.
    pub is_explicit: bool,
    /// Plays count string (e.g., "1.2M plays").
    #[allow(dead_code)]
    pub plays: Option<String>,
}

/// An album from an artist's page.
#[derive(Debug, Clone)]
pub struct ArtistAlbum {
    /// Album browse ID.
    pub browse_id: String,
    /// Album title.
    pub title: String,
    /// Release year.
    pub year: Option<String>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Album type (Album, Single, EP).
    #[allow(dead_code)]
    pub album_type: Option<String>,
    /// Whether this is an explicit album.
    pub is_explicit: bool,
}

/// A related artist.
#[derive(Debug, Clone)]
pub struct RelatedArtist {
    /// Artist browse ID.
    pub browse_id: String,
    /// Artist name.
    pub name: String,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Subscriber count string.
    pub subscribers: Option<String>,
}

/// Full artist page response.
#[derive(Debug, Clone, Default)]
pub struct ArtistPage {
    /// Artist details.
    pub details: Option<ArtistDetails>,
    /// Top tracks.
    pub top_tracks: Vec<ArtistTrack>,
    /// Albums.
    pub albums: Vec<ArtistAlbum>,
    /// Singles.
    pub singles: Vec<ArtistAlbum>,
    /// Related artists.
    pub related_artists: Vec<RelatedArtist>,
}

/// Get an artist's page with all their content.
///
/// # Arguments
///
/// * `client` - The YouTube Music API client
/// * `artist_id` - The artist's browse ID (channel ID starting with UC)
///
/// # Returns
///
/// The artist's page with top tracks, albums, singles, and related artists.
pub async fn get_artist(
    client: &YouTubeMusicClient,
    artist_id: &str,
) -> Result<ArtistPage, ClientError> {
    let body = json!({
        "browseId": artist_id
    });

    let response = client.post("browse", &body).await?;
    Ok(parse_artist_response(&response, artist_id))
}

/// Parse the artist page response.
fn parse_artist_response(response: &Value, artist_id: &str) -> ArtistPage {
    let mut page = ArtistPage {
        details: parse_artist_header(response, artist_id),
        ..Default::default()
    };

    // Find the content sections
    // Path: contents.singleColumnBrowseResultsRenderer.tabs[0].tabRenderer.content.sectionListRenderer.contents
    let contents = response
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|v| v.as_array());

    if let Some(sections) = contents {
        for section in sections {
            // Check for musicShelfRenderer (top songs)
            if let Some(shelf) = section.get("musicShelfRenderer") {
                let title = shelf
                    .pointer("/title/runs/0/text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if title.contains("Songs") || title.contains("songs") {
                    page.top_tracks = parse_top_tracks(shelf);
                }
            }

            // Check for musicCarouselShelfRenderer (albums, singles, related)
            if let Some(carousel) = section.get("musicCarouselShelfRenderer") {
                let title = carousel
                    .pointer("/header/musicCarouselShelfBasicHeaderRenderer/title/runs/0/text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if title.contains("Albums") || title.contains("albums") {
                    page.albums = parse_albums(carousel, Some("Album"));
                } else if title.contains("Singles") || title.contains("singles") {
                    page.singles = parse_albums(carousel, Some("Single"));
                } else if title.contains("Fans")
                    || title.contains("like")
                    || title.contains("Similar")
                {
                    page.related_artists = parse_related_artists(carousel);
                }
            }
        }
    }

    page
}

/// Parse artist header information.
fn parse_artist_header(response: &Value, artist_id: &str) -> Option<ArtistDetails> {
    // Try different header locations
    let header = response
        .get("header")
        .and_then(|h| h.get("musicImmersiveHeaderRenderer"))
        .or_else(|| {
            response
                .get("header")
                .and_then(|h| h.get("musicVisualHeaderRenderer"))
        })?;

    let name = header
        .pointer("/title/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    let thumbnail_url = header
        .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails")
        .and_then(|v| v.as_array())
        .and_then(|thumbs| thumbs.last())
        .and_then(|t| t.get("url"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let subscribers = header
        .pointer("/subscriptionButton/subscribeButtonRenderer/subscriberCountText/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from);

    let description = header
        .pointer("/description/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from);

    Some(ArtistDetails {
        browse_id: artist_id.to_string(),
        name,
        thumbnail_url,
        subscribers,
        description,
    })
}

/// Parse top tracks from a music shelf.
fn parse_top_tracks(shelf: &Value) -> Vec<ArtistTrack> {
    let mut tracks = Vec::new();

    let contents = shelf.get("contents").and_then(|v| v.as_array());

    if let Some(items) = contents {
        for item in items {
            if let Some(track) = parse_artist_track(item) {
                tracks.push(track);
            }
        }
    }

    tracks
}

/// Parse a single track from an artist page.
fn parse_artist_track(item: &Value) -> Option<ArtistTrack> {
    let renderer = item.get("musicResponsiveListItemRenderer")?;

    // Get video ID
    let video_id = renderer
        .pointer("/playlistItemData/videoId")
        .or_else(|| {
            renderer.pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId")
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

    // Second column: artists (and maybe plays)
    let second_column_runs = flex_columns
        .get(1)
        .and_then(|col| col.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
        .and_then(|v| v.as_array());

    let mut artists = Vec::new();
    let mut album: Option<AlbumRef> = None;
    let mut plays: Option<String> = None;

    if let Some(runs) = second_column_runs {
        for run in runs {
            let text = run.get("text").and_then(|v| v.as_str()).unwrap_or("");

            // Skip separators
            if text == " • " || text == " & " || text == ", " || text == " · " {
                continue;
            }

            // Check for plays count
            if text.contains("plays") || text.contains("play") {
                plays = Some(text.to_string());
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
            } else if !text.is_empty() && !text.contains(':') && !text.contains("plays") {
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

    Some(ArtistTrack {
        video_id,
        title,
        artists,
        album,
        duration_seconds,
        thumbnail_url,
        is_explicit,
        plays,
    })
}

/// Parse albums from a carousel.
fn parse_albums(carousel: &Value, album_type: Option<&str>) -> Vec<ArtistAlbum> {
    let mut albums = Vec::new();

    let contents = carousel.get("contents").and_then(|v| v.as_array());

    if let Some(items) = contents {
        for item in items {
            if let Some(album) = parse_artist_album(item, album_type) {
                albums.push(album);
            }
        }
    }

    albums
}

/// Parse a single album from an artist page carousel.
fn parse_artist_album(item: &Value, default_type: Option<&str>) -> Option<ArtistAlbum> {
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

    // Parse subtitle for year and album type
    let subtitle_runs = renderer
        .pointer("/subtitle/runs")
        .and_then(|v| v.as_array());

    let mut year: Option<String> = None;
    let mut album_type: Option<String> = default_type.map(String::from);

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

    Some(ArtistAlbum {
        browse_id,
        title,
        year,
        thumbnail_url,
        album_type,
        is_explicit,
    })
}

/// Parse related artists from a carousel.
fn parse_related_artists(carousel: &Value) -> Vec<RelatedArtist> {
    let mut artists = Vec::new();

    let contents = carousel.get("contents").and_then(|v| v.as_array());

    if let Some(items) = contents {
        for item in items {
            if let Some(artist) = parse_related_artist(item) {
                artists.push(artist);
            }
        }
    }

    artists
}

/// Parse a single related artist.
fn parse_related_artist(item: &Value) -> Option<RelatedArtist> {
    let renderer = item.get("musicTwoRowItemRenderer")?;

    // Get browse ID
    let browse_id = renderer
        .pointer("/navigationEndpoint/browseEndpoint/browseId")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Must be an artist (UC prefix)
    if !browse_id.starts_with("UC") {
        return None;
    }

    // Get name
    let name = renderer
        .pointer("/title/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // Get thumbnail
    let thumbnail_url = renderer
        .pointer("/thumbnailRenderer/musicThumbnailRenderer/thumbnail/thumbnails/0/url")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Get subscribers from subtitle
    let subscribers = renderer
        .pointer("/subtitle/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from);

    Some(RelatedArtist {
        browse_id,
        name,
        thumbnail_url,
        subscribers,
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

    fn mock_artist_response() -> Value {
        json!({
            "header": {
                "musicImmersiveHeaderRenderer": {
                    "title": {
                        "runs": [{"text": "Test Artist"}]
                    },
                    "thumbnail": {
                        "musicThumbnailRenderer": {
                            "thumbnail": {
                                "thumbnails": [
                                    {"url": "https://example.com/small.jpg"},
                                    {"url": "https://example.com/large.jpg"}
                                ]
                            }
                        }
                    },
                    "subscriptionButton": {
                        "subscribeButtonRenderer": {
                            "subscriberCountText": {
                                "runs": [{"text": "1.5M subscribers"}]
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
                                    "contents": [
                                        {
                                            "musicShelfRenderer": {
                                                "title": {
                                                    "runs": [{"text": "Songs"}]
                                                },
                                                "contents": [{
                                                    "musicResponsiveListItemRenderer": {
                                                        "playlistItemData": {
                                                            "videoId": "abc123"
                                                        },
                                                        "flexColumns": [
                                                            {
                                                                "musicResponsiveListItemFlexColumnRenderer": {
                                                                    "text": {
                                                                        "runs": [{"text": "Test Song"}]
                                                                    }
                                                                }
                                                            },
                                                            {
                                                                "musicResponsiveListItemFlexColumnRenderer": {
                                                                    "text": {
                                                                        "runs": [
                                                                            {
                                                                                "text": "Test Artist",
                                                                                "navigationEndpoint": {
                                                                                    "browseEndpoint": {
                                                                                        "browseId": "UC123"
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
                                                                    "runs": [{"text": "3:30"}]
                                                                }
                                                            }
                                                        }]
                                                    }
                                                }]
                                            }
                                        },
                                        {
                                            "musicCarouselShelfRenderer": {
                                                "header": {
                                                    "musicCarouselShelfBasicHeaderRenderer": {
                                                        "title": {
                                                            "runs": [{"text": "Albums"}]
                                                        }
                                                    }
                                                },
                                                "contents": [{
                                                    "musicTwoRowItemRenderer": {
                                                        "navigationEndpoint": {
                                                            "browseEndpoint": {
                                                                "browseId": "MPREb_album123"
                                                            }
                                                        },
                                                        "title": {
                                                            "runs": [{"text": "Test Album"}]
                                                        },
                                                        "subtitle": {
                                                            "runs": [
                                                                {"text": "Album"},
                                                                {"text": " • "},
                                                                {"text": "2023"}
                                                            ]
                                                        },
                                                        "thumbnailRenderer": {
                                                            "musicThumbnailRenderer": {
                                                                "thumbnail": {
                                                                    "thumbnails": [{
                                                                        "url": "https://example.com/album.jpg"
                                                                    }]
                                                                }
                                                            }
                                                        }
                                                    }
                                                }]
                                            }
                                        }
                                    ]
                                }
                            }
                        }
                    }]
                }
            }
        })
    }

    #[test]
    fn test_parse_artist_header() {
        let response = mock_artist_response();
        let details = parse_artist_header(&response, "UC123").unwrap();

        assert_eq!(details.name, "Test Artist");
        assert_eq!(details.browse_id, "UC123");
        assert!(details.thumbnail_url.is_some());
        assert_eq!(details.subscribers, Some("1.5M subscribers".to_string()));
    }

    #[test]
    fn test_parse_artist_response() {
        let response = mock_artist_response();
        let page = parse_artist_response(&response, "UC123");

        assert!(page.details.is_some());
        assert_eq!(page.details.as_ref().unwrap().name, "Test Artist");

        assert_eq!(page.top_tracks.len(), 1);
        assert_eq!(page.top_tracks[0].video_id, "abc123");
        assert_eq!(page.top_tracks[0].title, "Test Song");
        assert_eq!(page.top_tracks[0].duration_seconds, Some(210));

        assert_eq!(page.albums.len(), 1);
        assert_eq!(page.albums[0].browse_id, "MPREb_album123");
        assert_eq!(page.albums[0].title, "Test Album");
        assert_eq!(page.albums[0].year, Some("2023".to_string()));
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("3:30"), Some(210));
        assert_eq!(parse_duration("1:00:00"), Some(3600));
        assert_eq!(parse_duration("invalid"), None);
    }
}
