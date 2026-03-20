//! YouTube Music URL parsing.
//!
//! Parses URLs from music.youtube.com and youtube.com into their components.

use std::fmt;

use url::{Host, Url};

use crate::spotify::UriType;

/// A parsed YouTube Music URL.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct YouTubeUrl {
    /// The video/playlist/channel ID.
    pub id: String,
    /// The type of content.
    pub uri_type: UriType,
}

impl fmt::Display for YouTubeUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.uri_type {
            UriType::Track => write!(f, "https://music.youtube.com/watch?v={}", self.id),
            UriType::Album => write!(f, "https://music.youtube.com/browse/{}", self.id),
            UriType::Artist => write!(f, "https://music.youtube.com/channel/{}", self.id),
            UriType::Playlist => write!(f, "https://music.youtube.com/playlist?list={}", self.id),
            UriType::Show => write!(f, "https://music.youtube.com/browse/{}", self.id),
            UriType::Episode => write!(f, "https://music.youtube.com/watch?v={}", self.id),
        }
    }
}

impl YouTubeUrl {
    /// Create a new YouTubeUrl.
    pub fn new(id: &str, uri_type: UriType) -> Self {
        Self {
            id: id.to_string(),
            uri_type,
        }
    }

    /// Parse a YouTube Music or YouTube URL.
    ///
    /// Supports:
    /// - `https://music.youtube.com/watch?v=VIDEO_ID` - Track
    /// - `https://music.youtube.com/playlist?list=PLAYLIST_ID` - Playlist
    /// - `https://music.youtube.com/browse/MPREb_XXX` - Album (browse IDs starting with MPREb_)
    /// - `https://music.youtube.com/browse/VLPL...` - Playlist (browse IDs starting with VLPL)
    /// - `https://music.youtube.com/channel/CHANNEL_ID` - Artist
    /// - `https://www.youtube.com/watch?v=VIDEO_ID` - Track (regular YouTube)
    /// - `https://youtu.be/VIDEO_ID` - Track (short URL)
    pub fn from_url<S: AsRef<str>>(s: S) -> Option<Self> {
        let url = Url::parse(s.as_ref()).ok()?;

        match url.host() {
            Some(Host::Domain("music.youtube.com")) => Self::parse_music_youtube_url(&url),
            Some(Host::Domain("www.youtube.com")) | Some(Host::Domain("youtube.com")) => {
                Self::parse_youtube_url(&url)
            }
            Some(Host::Domain("youtu.be")) => Self::parse_short_url(&url),
            _ => None,
        }
    }

    /// Parse a music.youtube.com URL.
    fn parse_music_youtube_url(url: &Url) -> Option<Self> {
        let path = url.path();

        // Watch page: /watch?v=VIDEO_ID
        if path == "/watch" {
            let video_id = url
                .query_pairs()
                .find(|(k, _)| k == "v")
                .map(|(_, v)| v.to_string())?;
            return Some(Self::new(&video_id, UriType::Track));
        }

        // Playlist page: /playlist?list=PLAYLIST_ID
        if path == "/playlist" {
            let playlist_id = url
                .query_pairs()
                .find(|(k, _)| k == "list")
                .map(|(_, v)| v.to_string())?;
            return Some(Self::new(&playlist_id, UriType::Playlist));
        }

        // Browse page: /browse/ID
        if path.starts_with("/browse/") {
            let browse_id = path.strip_prefix("/browse/")?;
            // Album browse IDs typically start with "MPREb_"
            if browse_id.starts_with("MPREb_") {
                return Some(Self::new(browse_id, UriType::Album));
            }
            // Playlist browse IDs typically start with "VLPL" or "VL"
            if browse_id.starts_with("VL") {
                return Some(Self::new(browse_id, UriType::Playlist));
            }
            // UC prefix is for channels (artists)
            if browse_id.starts_with("UC") {
                return Some(Self::new(browse_id, UriType::Artist));
            }
            // Default to album for other browse IDs
            return Some(Self::new(browse_id, UriType::Album));
        }

        // Channel page: /channel/CHANNEL_ID
        if path.starts_with("/channel/") {
            let channel_id = path.strip_prefix("/channel/")?;
            return Some(Self::new(channel_id, UriType::Artist));
        }

        None
    }

    /// Parse a www.youtube.com URL.
    fn parse_youtube_url(url: &Url) -> Option<Self> {
        let path = url.path();

        // Watch page: /watch?v=VIDEO_ID
        if path == "/watch" {
            let video_id = url
                .query_pairs()
                .find(|(k, _)| k == "v")
                .map(|(_, v)| v.to_string())?;
            return Some(Self::new(&video_id, UriType::Track));
        }

        // Playlist page: /playlist?list=PLAYLIST_ID
        if path == "/playlist" {
            let playlist_id = url
                .query_pairs()
                .find(|(k, _)| k == "list")
                .map(|(_, v)| v.to_string())?;
            return Some(Self::new(&playlist_id, UriType::Playlist));
        }

        // Channel page: /channel/CHANNEL_ID
        if path.starts_with("/channel/") {
            let channel_id = path.strip_prefix("/channel/")?;
            return Some(Self::new(channel_id, UriType::Artist));
        }

        None
    }

    /// Parse a youtu.be short URL.
    fn parse_short_url(url: &Url) -> Option<Self> {
        // youtu.be/VIDEO_ID
        let video_id = url.path().strip_prefix('/')?;
        if !video_id.is_empty() {
            return Some(Self::new(video_id, UriType::Track));
        }
        None
    }

    /// Create a YouTubeUrl from a video ID.
    #[cfg(test)]
    pub fn from_video_id(video_id: &str) -> Self {
        Self::new(video_id, UriType::Track)
    }

    /// Create a YouTubeUrl from a playlist ID.
    #[cfg(test)]
    pub fn from_playlist_id(playlist_id: &str) -> Self {
        Self::new(playlist_id, UriType::Playlist)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_music_youtube_watch() {
        let url = YouTubeUrl::from_url("https://music.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
        assert_eq!(url.id, "dQw4w9WgXcQ");
        assert_eq!(url.uri_type, UriType::Track);
    }

    #[test]
    fn test_parse_music_youtube_playlist() {
        let url = YouTubeUrl::from_url(
            "https://music.youtube.com/playlist?list=PLrAXtmErZgOeiKm4sgNOknGvNjby9efdf",
        )
        .unwrap();
        assert_eq!(url.id, "PLrAXtmErZgOeiKm4sgNOknGvNjby9efdf");
        assert_eq!(url.uri_type, UriType::Playlist);
    }

    #[test]
    fn test_parse_music_youtube_album() {
        let url = YouTubeUrl::from_url("https://music.youtube.com/browse/MPREb_abc123xyz").unwrap();
        assert_eq!(url.id, "MPREb_abc123xyz");
        assert_eq!(url.uri_type, UriType::Album);
    }

    #[test]
    fn test_parse_music_youtube_channel() {
        let url =
            YouTubeUrl::from_url("https://music.youtube.com/channel/UCuAXFkgsw1L7xaCfnd5JJOw")
                .unwrap();
        assert_eq!(url.id, "UCuAXFkgsw1L7xaCfnd5JJOw");
        assert_eq!(url.uri_type, UriType::Artist);
    }

    #[test]
    fn test_parse_youtube_watch() {
        let url = YouTubeUrl::from_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
        assert_eq!(url.id, "dQw4w9WgXcQ");
        assert_eq!(url.uri_type, UriType::Track);
    }

    #[test]
    fn test_parse_short_url() {
        let url = YouTubeUrl::from_url("https://youtu.be/dQw4w9WgXcQ").unwrap();
        assert_eq!(url.id, "dQw4w9WgXcQ");
        assert_eq!(url.uri_type, UriType::Track);
    }

    #[test]
    fn test_parse_invalid_url() {
        assert!(YouTubeUrl::from_url("https://example.com/watch?v=abc").is_none());
        assert!(YouTubeUrl::from_url("not a url").is_none());
    }

    #[test]
    fn test_display() {
        let track = YouTubeUrl::from_video_id("dQw4w9WgXcQ");
        assert_eq!(
            track.to_string(),
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ"
        );

        let playlist = YouTubeUrl::from_playlist_id("PLabc123");
        assert_eq!(
            playlist.to_string(),
            "https://music.youtube.com/playlist?list=PLabc123"
        );
    }

    #[test]
    fn test_parse_browse_uc_channel() {
        let url = YouTubeUrl::from_url("https://music.youtube.com/browse/UCuAXFkgsw1L7xaCfnd5JJOw")
            .unwrap();
        assert_eq!(url.id, "UCuAXFkgsw1L7xaCfnd5JJOw");
        assert_eq!(url.uri_type, UriType::Artist);
    }

    #[test]
    fn test_parse_browse_vl_playlist() {
        let url = YouTubeUrl::from_url(
            "https://music.youtube.com/browse/VLPLrAXtmErZgOeiKm4sgNOknGvNjby9efdf",
        )
        .unwrap();
        assert_eq!(url.id, "VLPLrAXtmErZgOeiKm4sgNOknGvNjby9efdf");
        assert_eq!(url.uri_type, UriType::Playlist);
    }

    #[test]
    fn test_youtube_url_equality() {
        let url1 = YouTubeUrl::new("abc", UriType::Track);
        let url2 = YouTubeUrl::new("abc", UriType::Track);
        assert_eq!(url1, url2);

        let url3 = YouTubeUrl::new("abc", UriType::Album);
        assert_ne!(url1, url3);
    }
}
