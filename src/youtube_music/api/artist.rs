//! YouTube Music artist API.
//!
//! Provides access to artist details, top songs, and albums.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::search::{AlbumRef, ArtistRef};
use crate::youtube_music::{ClientError, YouTubeMusicClient};

/// Full artist details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artist {
    /// Artist browse ID (channel ID).
    pub browse_id: String,
    /// Artist name.
    pub name: String,
    /// Artist description/bio.
    pub description: Option<String>,
    /// Subscriber count (formatted string).
    pub subscribers: Option<String>,
    /// Total view count (formatted string).
    pub views: Option<String>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Top songs by this artist.
    pub top_songs: Vec<ArtistTopSong>,
    /// Albums by this artist.
    pub albums: Vec<ArtistAlbum>,
    /// Singles by this artist.
    pub singles: Vec<ArtistAlbum>,
    /// Channel ID for the artist.
    pub channel_id: Option<String>,
    /// Shuffle playlist ID (for "shuffle all" functionality).
    pub shuffle_playlist_id: Option<String>,
    /// Radio playlist ID.
    pub radio_playlist_id: Option<String>,
}

/// A top song by an artist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistTopSong {
    /// YouTube video ID.
    pub video_id: String,
    /// Song title.
    pub title: String,
    /// Album reference.
    pub album: Option<AlbumRef>,
    /// Play count (formatted string, e.g., "1.5B plays").
    pub plays: Option<String>,
    /// Duration in seconds.
    pub duration_seconds: Option<u32>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
}

/// An album in an artist's discography.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistAlbum {
    /// Album browse ID.
    pub browse_id: String,
    /// Album title.
    pub title: String,
    /// Release year.
    pub year: Option<String>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
    /// Whether this is explicit.
    pub is_explicit: bool,
}

/// Get artist details.
///
/// # Arguments
///
/// * `client` - The YouTube Music API client
/// * `browse_id` - The artist browse ID (usually starts with "UC")
///
/// # Returns
///
/// Artist details including top songs and albums.
pub async fn get_artist(
    client: &YouTubeMusicClient,
    browse_id: &str,
) -> Result<Artist, ClientError> {
    let body = json!({
        "browseId": browse_id
    });

    let response = client.post("browse", &body).await?;
    parse_artist_response(&response, browse_id)
}

/// Parse the artist response.
fn parse_artist_response(response: &Value, browse_id: &str) -> Result<Artist, ClientError> {
    // Extract header info
    let header = response.pointer("/header/musicImmersiveHeaderRenderer")
        .or_else(|| response.pointer("/header/musicVisualHeaderRenderer"));

    let name = header
        .and_then(|h| h.pointer("/title/runs/0/text"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| "Unknown Artist".to_string());

    let description = header
        .and_then(|h| h.pointer("/description/runs/0/text"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Subscriber count from subscription button
    let subscribers = header
        .and_then(|h| h.pointer("/subscriptionButton/subscribeButtonRenderer/subscriberCountText/runs/0/text"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Get thumbnail
    let thumbnail_url = header
        .and_then(|h| h.pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails"))
        .and_then(|thumbs| thumbs.as_array())
        .and_then(|arr| arr.last()) // Get highest quality
        .and_then(|t| t.get("url"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Get play button endpoints for shuffle/radio
    let shuffle_playlist_id = header
        .and_then(|h| h.pointer("/playButton/buttonRenderer/navigationEndpoint/watchEndpoint/playlistId"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let radio_playlist_id = header
        .and_then(|h| h.pointer("/startRadioButton/buttonRenderer/navigationEndpoint/watchEndpoint/playlistId"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Parse sections for songs and albums
    let sections = response
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|v| v.as_array());

    let mut top_songs = Vec::new();
    let mut albums = Vec::new();
    let mut singles = Vec::new();
    let mut views: Option<String> = None;

    if let Some(sections) = sections {
        for section in sections {
            // Check for musicShelfRenderer (songs)
            if let Some(shelf) = section.get("musicShelfRenderer") {
                let title = shelf
                    .pointer("/title/runs/0/text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if title.to_lowercase().contains("song") || title.to_lowercase().contains("top") {
                    if let Some(contents) = shelf.get("contents").and_then(|c| c.as_array()) {
                        for item in contents {
                            if let Some(song) = parse_artist_song(item) {
                                top_songs.push(song);
                            }
                        }
                    }
                }
            }

            // Check for musicCarouselShelfRenderer (albums, singles)
            if let Some(carousel) = section.get("musicCarouselShelfRenderer") {
                let title = carousel
                    .pointer("/header/musicCarouselShelfBasicHeaderRenderer/title/runs/0/text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if let Some(contents) = carousel.get("contents").and_then(|c| c.as_array()) {
                    for item in contents {
                        if let Some(album) = parse_artist_album(item) {
                            if title.to_lowercase().contains("single") {
                                singles.push(album);
                            } else if title.to_lowercase().contains("album") {
                                albums.push(album);
                            }
                        }
                    }
                }
            }

            // Check for musicDescriptionShelfRenderer (about section with views)
            if let Some(desc_shelf) = section.get("musicDescriptionShelfRenderer") {
                // Get view count from subheader
                if views.is_none() {
                    views = desc_shelf
                        .pointer("/subheader/runs/0/text")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                }
            }
        }
    }

    Ok(Artist {
        browse_id: browse_id.to_string(),
        name,
        description,
        subscribers,
        views,
        thumbnail_url,
        top_songs,
        albums,
        singles,
        channel_id: Some(browse_id.to_string()),
        shuffle_playlist_id,
        radio_playlist_id,
    })
}

/// Parse a song from artist's top songs.
fn parse_artist_song(item: &Value) -> Option<ArtistTopSong> {
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

    // Second column may contain plays count
    let plays = flex_columns
        .get(1)
        .and_then(|col| col.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text"))
        .and_then(|v| v.as_str())
        .filter(|s| s.to_lowercase().contains("play"))
        .map(String::from);

    // Look for album in flex columns (usually 3rd or 4th)
    let album = flex_columns
        .iter()
        .skip(1)
        .find_map(|col| {
            let runs = col.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs")?;
            let run = runs.as_array()?.first()?;
            let title = run.get("text").and_then(|v| v.as_str()).map(String::from)?;
            let browse_id = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str())
                .filter(|id| id.starts_with("MPREb"))
                .map(String::from);

            if browse_id.is_some() {
                Some(AlbumRef { title, browse_id })
            } else {
                None
            }
        });

    // Fixed column: duration
    let duration_seconds = renderer
        .pointer("/fixedColumns/0/musicResponsiveListItemFixedColumnRenderer/text/runs/0/text")
        .and_then(|v| v.as_str())
        .and_then(parse_duration);

    // Get thumbnail
    let thumbnail_url = renderer
        .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails/0/url")
        .and_then(|v| v.as_str())
        .map(String::from);

    Some(ArtistTopSong {
        video_id,
        title,
        album,
        plays,
        duration_seconds,
        thumbnail_url,
    })
}

/// Parse an album from artist's discography.
fn parse_artist_album(item: &Value) -> Option<ArtistAlbum> {
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

    // Get year from subtitle
    let year = renderer
        .pointer("/subtitle/runs")
        .and_then(|runs| runs.as_array())
        .and_then(|runs| {
            runs.iter().find_map(|run| {
                let text = run.get("text").and_then(|v| v.as_str())?;
                if text.len() == 4 && text.chars().all(|c| c.is_ascii_digit()) {
                    Some(text.to_string())
                } else {
                    None
                }
            })
        });

    // Get thumbnail
    let thumbnail_url = renderer
        .pointer("/thumbnailRenderer/musicThumbnailRenderer/thumbnail/thumbnails/0/url")
        .and_then(|v| v.as_str())
        .map(String::from);

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
        is_explicit,
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

    fn mock_artist_response() -> Value {
        json!({
            "header": {
                "musicImmersiveHeaderRenderer": {
                    "title": {
                        "runs": [{"text": "Rick Astley"}]
                    },
                    "description": {
                        "runs": [{"text": "English singer and songwriter"}]
                    },
                    "subscriptionButton": {
                        "subscribeButtonRenderer": {
                            "subscriberCountText": {
                                "runs": [{"text": "2.5M subscribers"}]
                            }
                        }
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
                    "playButton": {
                        "buttonRenderer": {
                            "navigationEndpoint": {
                                "watchEndpoint": {
                                    "playlistId": "RDCLAK5uy_shuffle123"
                                }
                            }
                        }
                    },
                    "startRadioButton": {
                        "buttonRenderer": {
                            "navigationEndpoint": {
                                "watchEndpoint": {
                                    "playlistId": "RDCLAK5uy_radio123"
                                }
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
                                                                        "runs": [{"text": "1.5B plays"}]
                                                                    }
                                                                }
                                                            },
                                                            {
                                                                "musicResponsiveListItemFlexColumnRenderer": {
                                                                    "text": {
                                                                        "runs": [{
                                                                            "text": "Whenever You Need Somebody",
                                                                            "navigationEndpoint": {
                                                                                "browseEndpoint": {
                                                                                    "browseId": "MPREb_album123"
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
                                                        }],
                                                        "thumbnail": {
                                                            "musicThumbnailRenderer": {
                                                                "thumbnail": {
                                                                    "thumbnails": [{
                                                                        "url": "https://example.com/song_thumb.jpg"
                                                                    }]
                                                                }
                                                            }
                                                        }
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
                                                            "runs": [{"text": "Whenever You Need Somebody"}]
                                                        },
                                                        "subtitle": {
                                                            "runs": [
                                                                {"text": "Album"},
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
                                        },
                                        {
                                            "musicCarouselShelfRenderer": {
                                                "header": {
                                                    "musicCarouselShelfBasicHeaderRenderer": {
                                                        "title": {
                                                            "runs": [{"text": "Singles"}]
                                                        }
                                                    }
                                                },
                                                "contents": [{
                                                    "musicTwoRowItemRenderer": {
                                                        "navigationEndpoint": {
                                                            "browseEndpoint": {
                                                                "browseId": "MPREb_single456"
                                                            }
                                                        },
                                                        "title": {
                                                            "runs": [{"text": "Together Forever"}]
                                                        },
                                                        "subtitle": {
                                                            "runs": [
                                                                {"text": "Single"},
                                                                {"text": " • "},
                                                                {"text": "1988"}
                                                            ]
                                                        }
                                                    }
                                                }]
                                            }
                                        },
                                        {
                                            "musicDescriptionShelfRenderer": {
                                                "subheader": {
                                                    "runs": [{"text": "5.2B views"}]
                                                }
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

    fn mock_empty_artist_response() -> Value {
        json!({
            "header": {
                "musicImmersiveHeaderRenderer": {
                    "title": {
                        "runs": [{"text": "Unknown Artist"}]
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
    fn test_parse_artist_header() {
        let response = mock_artist_response();
        let artist = parse_artist_response(&response, "UCuAXFkgsw1L7xaCfnd5JJOw").unwrap();

        assert_eq!(artist.browse_id, "UCuAXFkgsw1L7xaCfnd5JJOw");
        assert_eq!(artist.name, "Rick Astley");
        assert_eq!(artist.description, Some("English singer and songwriter".to_string()));
        assert_eq!(artist.subscribers, Some("2.5M subscribers".to_string()));
        assert_eq!(artist.views, Some("5.2B views".to_string()));
        assert_eq!(artist.thumbnail_url, Some("https://example.com/large.jpg".to_string())); // Should get largest
        assert_eq!(artist.shuffle_playlist_id, Some("RDCLAK5uy_shuffle123".to_string()));
        assert_eq!(artist.radio_playlist_id, Some("RDCLAK5uy_radio123".to_string()));
    }

    #[test]
    fn test_parse_artist_top_songs() {
        let response = mock_artist_response();
        let artist = parse_artist_response(&response, "UCuAXFkgsw1L7xaCfnd5JJOw").unwrap();

        assert_eq!(artist.top_songs.len(), 1);
        let song = &artist.top_songs[0];
        assert_eq!(song.video_id, "dQw4w9WgXcQ");
        assert_eq!(song.title, "Never Gonna Give You Up");
        assert_eq!(song.plays, Some("1.5B plays".to_string()));
        assert_eq!(song.duration_seconds, Some(213)); // 3:33
        assert!(song.album.is_some());
        assert_eq!(song.album.as_ref().unwrap().title, "Whenever You Need Somebody");
        assert!(song.thumbnail_url.is_some());
    }

    #[test]
    fn test_parse_artist_albums() {
        let response = mock_artist_response();
        let artist = parse_artist_response(&response, "UCuAXFkgsw1L7xaCfnd5JJOw").unwrap();

        assert_eq!(artist.albums.len(), 1);
        let album = &artist.albums[0];
        assert_eq!(album.browse_id, "MPREb_album123");
        assert_eq!(album.title, "Whenever You Need Somebody");
        assert_eq!(album.year, Some("1987".to_string()));
        assert!(album.thumbnail_url.is_some());
    }

    #[test]
    fn test_parse_artist_singles() {
        let response = mock_artist_response();
        let artist = parse_artist_response(&response, "UCuAXFkgsw1L7xaCfnd5JJOw").unwrap();

        assert_eq!(artist.singles.len(), 1);
        let single = &artist.singles[0];
        assert_eq!(single.browse_id, "MPREb_single456");
        assert_eq!(single.title, "Together Forever");
        assert_eq!(single.year, Some("1988".to_string()));
    }

    #[test]
    fn test_parse_empty_artist() {
        let response = mock_empty_artist_response();
        let artist = parse_artist_response(&response, "empty_id").unwrap();

        assert_eq!(artist.name, "Unknown Artist");
        assert!(artist.top_songs.is_empty());
        assert!(artist.albums.is_empty());
        assert!(artist.singles.is_empty());
    }

    #[test]
    fn test_artist_serialization() {
        let artist = Artist {
            browse_id: "UC123".to_string(),
            name: "Test Artist".to_string(),
            description: Some("A test artist".to_string()),
            subscribers: Some("1M".to_string()),
            views: Some("10B".to_string()),
            thumbnail_url: None,
            top_songs: vec![],
            albums: vec![],
            singles: vec![],
            channel_id: Some("UC123".to_string()),
            shuffle_playlist_id: None,
            radio_playlist_id: None,
        };

        let json = serde_json::to_string(&artist).unwrap();
        let deserialized: Artist = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.browse_id, artist.browse_id);
        assert_eq!(deserialized.name, artist.name);
        assert_eq!(deserialized.subscribers, artist.subscribers);
    }
}
