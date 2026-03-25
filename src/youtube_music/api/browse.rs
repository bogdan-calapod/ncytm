//! YouTube Music browse/explore API.
//!
//! Provides access to mood/genre categories and their associated playlists,
//! powering the Browse tab.

use serde_json::{Value, json};

use crate::youtube_music::{ClientError, YouTubeMusicClient};

/// A browse category (mood, genre, etc.).
#[derive(Debug, Clone)]
pub struct BrowseCategory {
    /// Browse ID for this category.
    pub browse_id: String,
    /// Display name.
    pub name: String,
    /// Thumbnail URL.
    #[allow(dead_code)]
    pub thumbnail_url: Option<String>,
}

/// A playlist returned from a category browse.
#[derive(Debug, Clone)]
pub struct CategoryPlaylist {
    /// Playlist ID.
    pub playlist_id: String,
    /// Playlist title.
    pub title: String,
    /// Subtitle / description excerpt.
    pub subtitle: Option<String>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
}

/// Fetch the top-level mood/genre categories from the YouTube Music Explore page.
pub async fn get_categories(
    client: &YouTubeMusicClient,
) -> Result<Vec<BrowseCategory>, ClientError> {
    let body = json!({ "browseId": "FEmusic_explore" });
    let response = client.post("browse", &body).await?;
    Ok(parse_categories(&response))
}

/// Fetch playlists for a given category browse ID.
pub async fn get_category_playlists(
    client: &YouTubeMusicClient,
    browse_id: &str,
) -> Result<Vec<CategoryPlaylist>, ClientError> {
    let body = json!({ "browseId": browse_id });
    let response = client.post("browse", &body).await?;
    Ok(parse_category_playlists(&response))
}

// ── Parsers ───────────────────────────────────────────────────────────────────

fn parse_categories(response: &Value) -> Vec<BrowseCategory> {
    let mut categories = Vec::new();

    // Path: contents.singleColumnBrowseResultsRenderer.tabs[0].tabRenderer.content
    //       .sectionListRenderer.contents[*].musicCarouselShelfRenderer
    let contents = response
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|v| v.as_array());

    let sections = match contents {
        Some(s) => s,
        None => return categories,
    };

    for section in sections {
        // Each section may be a musicCarouselShelfRenderer (e.g., "Moods & genres")
        let carousel = match section.get("musicCarouselShelfRenderer") {
            Some(c) => c,
            None => continue,
        };

        let section_title = carousel
            .pointer("/header/musicCarouselShelfBasicHeaderRenderer/title/runs/0/text")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // We're interested in mood/genre sections
        let is_mood_genre = section_title.to_lowercase().contains("mood")
            || section_title.to_lowercase().contains("genre")
            || section_title.to_lowercase().contains("explore")
            || section_title.is_empty();

        if !is_mood_genre {
            continue;
        }

        if let Some(items) = carousel.get("contents").and_then(|v| v.as_array()) {
            for item in items {
                if let Some(cat) = parse_category_item(item) {
                    categories.push(cat);
                }
            }
        }
    }

    // If the carousel filtering yielded nothing, fall back to collecting all
    // musicTwoRowItemRenderer items regardless of section
    if categories.is_empty() {
        for section in sections {
            let carousel = match section.get("musicCarouselShelfRenderer") {
                Some(c) => c,
                None => continue,
            };
            if let Some(items) = carousel.get("contents").and_then(|v| v.as_array()) {
                for item in items {
                    if let Some(cat) = parse_category_item(item) {
                        categories.push(cat);
                    }
                }
            }
        }
    }

    categories
}

fn parse_category_item(item: &Value) -> Option<BrowseCategory> {
    let renderer = item.get("musicTwoRowItemRenderer")?;

    let browse_id = renderer
        .pointer("/navigationEndpoint/browseEndpoint/browseId")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    let name = renderer
        .pointer("/title/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    let thumbnail_url = renderer
        .pointer("/thumbnailRenderer/musicThumbnailRenderer/thumbnail/thumbnails/0/url")
        .and_then(|v| v.as_str())
        .map(String::from);

    Some(BrowseCategory {
        browse_id,
        name,
        thumbnail_url,
    })
}

fn parse_category_playlists(response: &Value) -> Vec<CategoryPlaylist> {
    let mut playlists = Vec::new();

    let contents = response
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|v| v.as_array());

    let sections = match contents {
        Some(s) => s,
        None => return playlists,
    };

    for section in sections {
        let carousel = match section.get("musicCarouselShelfRenderer") {
            Some(c) => c,
            None => continue,
        };

        if let Some(items) = carousel.get("contents").and_then(|v| v.as_array()) {
            for item in items {
                if let Some(pl) = parse_category_playlist(item) {
                    playlists.push(pl);
                }
            }
        }
    }

    playlists
}

fn parse_category_playlist(item: &Value) -> Option<CategoryPlaylist> {
    let renderer = item.get("musicTwoRowItemRenderer")?;

    // Navigate to playlist ID; it may use a watchEndpoint (for auto-generated playlists)
    // or a browseEndpoint (for regular playlists).
    let playlist_id = renderer
        .pointer("/navigationEndpoint/watchEndpoint/playlistId")
        .or_else(|| renderer.pointer("/navigationEndpoint/watchPlaylistEndpoint/playlistId"))
        .or_else(|| renderer.pointer("/navigationEndpoint/browseEndpoint/browseId"))
        .and_then(|v| v.as_str())
        .map(|s| s.strip_prefix("VL").unwrap_or(s).to_string())?;

    let title = renderer
        .pointer("/title/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from)?;

    let subtitle = renderer
        .pointer("/subtitle/runs/0/text")
        .and_then(|v| v.as_str())
        .map(String::from);

    let thumbnail_url = renderer
        .pointer("/thumbnailRenderer/musicThumbnailRenderer/thumbnail/thumbnails/0/url")
        .and_then(|v| v.as_str())
        .map(String::from);

    Some(CategoryPlaylist {
        playlist_id,
        title,
        subtitle,
        thumbnail_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_explore_response() -> Value {
        json!({
            "contents": {
                "singleColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicCarouselShelfRenderer": {
                                            "header": {
                                                "musicCarouselShelfBasicHeaderRenderer": {
                                                    "title": { "runs": [{"text": "Moods & genres"}] }
                                                }
                                            },
                                            "contents": [
                                                {
                                                    "musicTwoRowItemRenderer": {
                                                        "navigationEndpoint": {
                                                            "browseEndpoint": {"browseId": "FEmusic_moods_and_genres_nc"}
                                                        },
                                                        "title": { "runs": [{"text": "Chill"}] },
                                                        "thumbnailRenderer": {
                                                            "musicThumbnailRenderer": {
                                                                "thumbnail": {
                                                                    "thumbnails": [{"url": "https://example.com/chill.jpg"}]
                                                                }
                                                            }
                                                        }
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

    fn mock_category_playlists_response() -> Value {
        json!({
            "contents": {
                "singleColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicCarouselShelfRenderer": {
                                            "contents": [{
                                                "musicTwoRowItemRenderer": {
                                                    "navigationEndpoint": {
                                                        "watchPlaylistEndpoint": {
                                                            "playlistId": "RDCLAK5uy_abc123"
                                                        }
                                                    },
                                                    "title": { "runs": [{"text": "Chill Vibes"}] },
                                                    "subtitle": { "runs": [{"text": "Playlist"}] },
                                                    "thumbnailRenderer": {
                                                        "musicThumbnailRenderer": {
                                                            "thumbnail": {
                                                                "thumbnails": [{"url": "https://example.com/pl.jpg"}]
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
    fn test_parse_categories() {
        let response = mock_explore_response();
        let cats = parse_categories(&response);
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].name, "Chill");
        assert_eq!(cats[0].browse_id, "FEmusic_moods_and_genres_nc");
        assert!(cats[0].thumbnail_url.is_some());
    }

    #[test]
    fn test_parse_category_playlists() {
        let response = mock_category_playlists_response();
        let pls = parse_category_playlists(&response);
        assert_eq!(pls.len(), 1);
        assert_eq!(pls[0].title, "Chill Vibes");
        assert_eq!(pls[0].playlist_id, "RDCLAK5uy_abc123");
        assert!(pls[0].thumbnail_url.is_some());
    }
}
