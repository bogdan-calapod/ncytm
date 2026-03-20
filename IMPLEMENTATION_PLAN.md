# ncytm Implementation Plan

## Overview

Direct replacement of Spotify backend with YouTube Music. No abstraction layer - just swap implementations.

**Development Approach**: Test-Driven Development (TDD)
- Write tests first, validate them, then implement
- One commit per logical change
- Each commit leaves codebase buildable
- **User validates and commits** - AI prepares changes, shows diff, suggests commit message

---

## Phase 0: Foundation
*Goal: Fix build, establish baseline*

### 0.1 Rename crate references in code
**Task**: Update `extern crate ncspot` → `ncytm` everywhere
**Test**: `cargo build` succeeds (even if functionality is broken)
**Commit**: "chore: rename crate references from ncspot to ncytm"

### 0.2 Remove Spotify dependencies from Cargo.toml
**Task**: Remove librespot-*, rspotify crates; add reqwest, serde_json
**Test**: `cargo check` passes (with dead code warnings)
**Commit**: "chore: swap Spotify deps for HTTP client deps"

### 0.3 Stub out Spotify modules
**Task**: Replace Spotify module contents with `todo!()` or empty implementations
**Test**: `cargo build` succeeds
**Commit**: "chore: stub Spotify modules for replacement"

---

## Phase 1: YouTube Music Authentication
*Goal: Cookie-based authentication working*

### 1.1 Cookie parser
**File**: `src/youtube_music/cookies.rs` (new)
**Task**: Parse cookies from Netscape format file

```rust
pub struct Cookies {
    jar: HashMap<String, String>,
}

impl Cookies {
    pub fn from_file(path: &Path) -> Result<Self, Error>;
    pub fn get(&self, name: &str) -> Option<&str>;
    pub fn has_required(&self) -> bool;  // Check SAPISID, etc.
}
```

**Tests**:
- Parse valid Netscape cookie file
- Detect missing required cookies
- Handle malformed input

**Commit**: "feat: add cookie parser for YouTube Music auth"

### 1.2 SAPISID hash generation
**File**: `src/youtube_music/auth.rs` (new)
**Task**: Generate SAPISIDHASH for API authentication

```rust
pub fn generate_sapisid_hash(sapisid: &str, origin: &str) -> String;
```

**Tests**:
- Known input produces expected hash
- Hash format is correct (SAPISIDHASH timestamp_hash)

**Commit**: "feat: add SAPISID hash generation"

### 1.3 YouTube Music API client base
**File**: `src/youtube_music/client.rs` (new)
**Task**: HTTP client with auth headers

```rust
pub struct YouTubeMusicClient {
    http: reqwest::Client,
    cookies: Cookies,
}

impl YouTubeMusicClient {
    pub fn new(cookies: Cookies) -> Self;
    pub async fn post(&self, endpoint: &str, body: &Value) -> Result<Value, Error>;
}
```

**Tests**:
- Headers include auth cookies
- Headers include SAPISID hash
- Request body has correct context structure

**Commit**: "feat: add YouTube Music API client"

### 1.4 Authentication verification
**File**: `src/youtube_music/client.rs` (modify)
**Task**: Verify cookies work by fetching account info

```rust
impl YouTubeMusicClient {
    pub async fn verify_auth(&self) -> Result<AccountInfo, Error>;
}
```

**Tests**:
- Returns account info on valid cookies
- Returns error on invalid/expired cookies

**Commit**: "feat: add authentication verification"

---

## Phase 2: Core API Endpoints
*Goal: Search, library, and content fetching*

### 2.1 Search endpoint
**File**: `src/youtube_music/api/search.rs` (new)
**Task**: Implement search functionality

```rust
pub async fn search(client: &YouTubeMusicClient, query: &str) -> Result<SearchResults, Error>;
```

**Tests**:
- Parse tracks from search response
- Parse albums from search response
- Parse artists from search response
- Handle empty results

**Commit**: "feat: add YouTube Music search API"

### 2.2 Get library - liked songs
**File**: `src/youtube_music/api/library.rs` (new)
**Task**: Fetch user's liked songs

```rust
pub async fn get_liked_songs(client: &YouTubeMusicClient) -> Result<Vec<Track>, Error>;
```

**Tests**:
- Parse liked songs list
- Handle pagination/continuation

**Commit**: "feat: add liked songs API"

### 2.3 Get library - playlists
**File**: `src/youtube_music/api/library.rs` (modify)
**Task**: Fetch user's playlists

```rust
pub async fn get_library_playlists(client: &YouTubeMusicClient) -> Result<Vec<Playlist>, Error>;
```

**Tests**:
- Parse playlist list
- Include playlist metadata (track count, etc.)

**Commit**: "feat: add library playlists API"

### 2.4 Get library - albums
**File**: `src/youtube_music/api/library.rs` (modify)
**Task**: Fetch user's saved albums

```rust
pub async fn get_library_albums(client: &YouTubeMusicClient) -> Result<Vec<Album>, Error>;
```

**Tests**:
- Parse album list

**Commit**: "feat: add library albums API"

### 2.5 Get playlist details
**File**: `src/youtube_music/api/playlist.rs` (new)
**Task**: Fetch full playlist with tracks

```rust
pub async fn get_playlist(client: &YouTubeMusicClient, id: &str) -> Result<Playlist, Error>;
```

**Tests**:
- Parse playlist tracks
- Handle continuation tokens for large playlists

**Commit**: "feat: add playlist details API"

### 2.6 Get album details
**File**: `src/youtube_music/api/album.rs` (new)
**Task**: Fetch album with tracks

```rust
pub async fn get_album(client: &YouTubeMusicClient, id: &str) -> Result<Album, Error>;
```

**Tests**:
- Parse album tracks
- Parse album metadata

**Commit**: "feat: add album details API"

### 2.7 Get artist details
**File**: `src/youtube_music/api/artist.rs` (new)
**Task**: Fetch artist info

```rust
pub async fn get_artist(client: &YouTubeMusicClient, id: &str) -> Result<Artist, Error>;
```

**Tests**:
- Parse artist info
- Parse top songs, albums

**Commit**: "feat: add artist details API"

---

## Phase 3: Models
*Goal: Update existing models for YouTube Music data*

### 3.1 Update Track model
**File**: `src/model/track.rs` (modify)
**Task**: Change fields to match YouTube Music data

Changes:
- `id: String` - YouTube video ID
- Remove Spotify-specific fields (uri, disc_number, etc.)
- Add `video_id: String`
- Keep `title`, `artists`, `album`, `duration`

**Tests**:
- Track creation from YouTube data
- ListItem trait still works
- Display formatting

**Commit**: "feat: update Track model for YouTube Music"

### 3.2 Update Album model
**File**: `src/model/album.rs` (modify)
**Task**: Change fields for YouTube Music

Changes:
- `id: String` - YouTube browse ID
- Remove Spotify URIs
- Keep `title`, `artists`, `tracks`, `cover_url`

**Tests**:
- Album creation
- ListItem trait works

**Commit**: "feat: update Album model for YouTube Music"

### 3.3 Update Artist model
**File**: `src/model/artist.rs` (modify)
**Task**: Change fields for YouTube Music

**Tests**:
- Artist creation
- ListItem trait works

**Commit**: "feat: update Artist model for YouTube Music"

### 3.4 Update Playlist model
**File**: `src/model/playlist.rs` (modify)
**Task**: Change fields for YouTube Music

**Tests**:
- Playlist creation
- ListItem trait works

**Commit**: "feat: update Playlist model for YouTube Music"

### 3.5 Remove Episode/Show models (optional)
**File**: `src/model/episode.rs`, `src/model/show.rs`
**Task**: Remove podcast support (YouTube Music doesn't have podcasts in the same way)

**Commit**: "chore: remove podcast models"

### 3.6 Update Playable enum
**File**: `src/model/playable.rs` (modify)
**Task**: Simplify to just Track (or Track | Video if we want music videos)

**Tests**:
- Playable works with Track

**Commit**: "feat: simplify Playable enum"

---

## Phase 4: Audio Playback
*Goal: Play audio from YouTube*

### 4.1 Stream URL extraction
**File**: `src/youtube_music/stream.rs` (new)
**Task**: Get playable audio URL from video ID

Options (pick one):
- `rusty_ytdl` crate (pure Rust)
- Shell out to `yt-dlp`

```rust
pub async fn get_stream_url(video_id: &str) -> Result<StreamInfo, Error>;

pub struct StreamInfo {
    pub url: String,
    pub format: String,  // "audio/webm", "audio/mp4", etc.
    pub expires_at: Option<SystemTime>,
}
```

**Tests**:
- Extract URL for known video
- Handle unavailable videos
- Handle age-restricted content

**Commit**: "feat: add YouTube stream URL extraction"

### 4.2 Audio player
**File**: `src/player.rs` (new, replaces spotify_worker.rs)
**Task**: Play audio from URL using rodio/symphonia

```rust
pub struct Player {
    sink: rodio::Sink,
    // ...
}

impl Player {
    pub fn new() -> Self;
    pub fn load(&mut self, url: &str) -> Result<(), Error>;
    pub fn play(&mut self);
    pub fn pause(&mut self);
    pub fn stop(&mut self);
    pub fn seek(&mut self, position: Duration);
    pub fn set_volume(&mut self, volume: f32);
    pub fn position(&self) -> Duration;
}
```

**Tests**:
- Load and play audio
- Pause/resume
- Volume control
- Position tracking

**Commit**: "feat: add audio player"

### 4.3 Player worker thread
**File**: `src/player_worker.rs` (new)
**Task**: Background thread managing playback (mirrors spotify_worker.rs pattern)

```rust
pub enum PlayerCommand {
    Load { video_id: String, start_playing: bool },
    Play,
    Pause,
    Stop,
    Seek(Duration),
    SetVolume(f32),
    Shutdown,
}

pub enum PlayerEvent {
    Playing(SystemTime),
    Paused(Duration),
    Stopped,
    FinishedTrack,
    Error(String),
}
```

**Tests**:
- Worker processes commands
- Worker emits events
- Clean shutdown

**Commit**: "feat: add player worker thread"

---

## Phase 5: Integration
*Goal: Wire everything together*

### 5.1 Replace spotify.rs with youtube.rs
**File**: `src/youtube.rs` (new, replaces `src/spotify.rs`)
**Task**: High-level YouTube Music interface

```rust
pub struct YouTube {
    client: YouTubeMusicClient,
    player_tx: Sender<PlayerCommand>,
    // ...
}

impl YouTube {
    pub fn new(cookies_path: &Path) -> Result<Self, Error>;
    pub fn play(&self, track: &Track);
    pub fn pause(&self);
    // ... mirror spotify.rs interface
}
```

**Tests**:
- Integration with player
- Command forwarding

**Commit**: "feat: add YouTube Music controller"

### 5.2 Update authentication.rs
**File**: `src/authentication.rs` (rewrite)
**Task**: Cookie-based auth flow

```rust
pub fn authenticate() -> Result<Cookies, Error> {
    // 1. Check for cookies file
    // 2. If not found, prompt user to provide cookies
    // 3. Validate cookies work
    // 4. Return cookies
}
```

**Tests**:
- Load existing cookies
- Handle missing cookies file
- Handle invalid cookies

**Commit**: "feat: update authentication for YouTube Music"

### 5.3 Update config.rs
**File**: `src/config.rs` (modify)
**Task**: Add YouTube Music config options

```toml
# ~/.config/ncytm/config.toml
cookies_file = "cookies.txt"  # Path to cookies file
```

Remove Spotify-specific options (bitrate, ap_port, etc.)

**Tests**:
- Parse new config
- Default values work

**Commit**: "feat: update config for YouTube Music"

### 5.4 Update library.rs
**File**: `src/library.rs` (modify)
**Task**: Fetch library from YouTube Music

**Tests**:
- Load library tracks
- Load playlists
- Caching works

**Commit**: "feat: update library for YouTube Music"

### 5.5 Update application.rs
**File**: `src/application.rs` (modify)
**Task**: Use YouTube instead of Spotify

**Tests**:
- App initializes
- Basic flow works

**Commit**: "feat: integrate YouTube Music into application"

### 5.6 Update URL handling
**File**: `src/spotify_url.rs` → `src/youtube_url.rs`
**Task**: Parse YouTube Music URLs

```rust
pub fn parse_url(url: &str) -> Option<YouTubeItem>;

pub enum YouTubeItem {
    Track(String),      // video ID
    Playlist(String),   // playlist ID
    Album(String),      // browse ID
    Artist(String),     // channel ID
}
```

**Tests**:
- Parse music.youtube.com URLs
- Parse youtube.com URLs
- Parse youtu.be short URLs

**Commit**: "feat: add YouTube URL parsing"

---

## Phase 6: Cleanup
*Goal: Remove dead code, polish*

### 6.1 Delete Spotify files
**Task**: Remove `src/spotify.rs`, `src/spotify_api.rs`, `src/spotify_worker.rs`, `src/spotify_url.rs`

**Commit**: "chore: remove Spotify implementation files"

### 6.2 Update UI components
**Files**: `src/ui/*.rs`
**Task**: Remove any Spotify-specific UI code (podcast tabs, etc.)

**Commit**: "feat: clean up UI for YouTube Music"

### 6.3 Update sharing.rs
**Task**: Generate YouTube Music share URLs

**Commit**: "feat: update sharing for YouTube Music"

### 6.4 Update mpris.rs
**Task**: Correct metadata for MPRIS

**Commit**: "feat: update MPRIS for YouTube Music"

### 6.5 Update IPC
**File**: `src/ipc.rs`
**Task**: Update socket name, JSON structure

**Commit**: "feat: update IPC for YouTube Music"

### 6.6 Final cleanup
**Task**: Remove dead code, update comments, fix warnings

**Commit**: "chore: final cleanup"

---

## Summary

| Phase | Tasks | Description |
|-------|-------|-------------|
| 0 | 3 | Fix build |
| 1 | 4 | Authentication |
| 2 | 7 | API endpoints |
| 3 | 6 | Update models |
| 4 | 3 | Audio playback |
| 5 | 6 | Integration |
| 6 | 6 | Cleanup |

**Total: 35 tasks**

Much simpler than the abstraction approach - direct replacement, ~30% fewer tasks.

---

## Key Files Mapping

| Spotify (old) | YouTube Music (new) |
|---------------|---------------------|
| `spotify.rs` | `youtube.rs` |
| `spotify_api.rs` | `youtube_music/client.rs` + `youtube_music/api/*.rs` |
| `spotify_worker.rs` | `player_worker.rs` |
| `spotify_url.rs` | `youtube_url.rs` |
| `authentication.rs` | `authentication.rs` (rewritten) |

---

## Dependencies

**Remove:**
```toml
librespot-core = "0.8.0"
librespot-oauth = "0.8.0"
librespot-playback = "0.8.0"
librespot-protocol = "0.8.0"
rspotify = "0.15.0"
```

**Add:**
```toml
reqwest = { version = "0.12", features = ["json", "cookies", "stream"] }
sha1 = "0.10"  # For SAPISID hash
rusty_ytdl = "0.7"  # Or yt-dlp subprocess
rodio = { version = "0.19", features = ["symphonia-all"] }
```
