# ncytm Project Skill File

## Project Overview

ncytm is a fork of [ncspot](https://github.com/hrkfdn/ncspot), an ncurses Spotify client written in Rust. The goal is to convert it to use YouTube Music as a backend instead of Spotify, using cookie-based authentication.

**AI Involvement**: This project is being developed with heavy AI assistance. All code changes should be documented and testable.

## Development Principles

### 1. Test-Driven Development (TDD)
- **Write tests FIRST**, validate them, then implement
- Tests should be validated before considering them done
- Implementation should be driven by unit tests
- Each test should verify a specific, isolated piece of functionality

### 2. Incremental Changes
- Make small, bite-sized changes that can be committed one-by-one
- Each change should be testable and build on previous changes
- Changes should be independently verifiable
- Avoid large, sweeping refactors - prefer gradual migration

### 3. Commit Strategy
- One logical change per commit
- Commits should leave the codebase in a working state
- Write descriptive commit messages explaining WHY, not just WHAT
- **AI should run `git add` and `git commit`** after making changes to trigger linting/formatting hooks
- Use **Conventional Commits** format for commit messages:
  - `feat: <description>` - New feature
  - `fix: <description>` - Bug fix
  - `refactor: <description>` - Code refactoring
  - `docs: <description>` - Documentation changes
  - `test: <description>` - Test additions/changes
  - `chore: <description>` - Maintenance tasks
  - `style: <description>` - Formatting, styling changes
- Husky pre-commit hooks will run automatically to ensure code quality

### 4. Documentation
- Update this skill file when new principles are established
- Document architectural decisions
- Keep README and docs updated with current state

## Architecture Overview

### Current Structure (Spotify-based)

```
Application
    │
    ├── UI Layer (cursive-based TUI)
    │   └── Uses ListItem trait for display
    │
    ├── Core Layer
    │   ├── Queue (playback order, shuffle, repeat)
    │   ├── Library (user's saved content, caching)
    │   └── Spotify (high-level playback control)
    │
    └── Backend Layer
        ├── WebApi (rspotify → Spotify Web API)
        └── Worker (librespot → audio playback)
```

### Key Abstractions

**`ListItem` trait** (`src/traits.rs`) - Core abstraction for displayable/playable items:
- `Track`, `Album`, `Artist`, `Playlist`, `Show`, `Episode` implement this
- UI components are generic over `ListItem`

**`Playable` enum** (`src/model/playable.rs`) - Anything that can be played:
- Currently: `Track` | `Episode`
- For YouTube Music: `Track` (songs) | potentially `Video`

### Components to Replace

| Component | Current | Target | Coupling |
|-----------|---------|--------|----------|
| `authentication.rs` | OAuth2 (librespot) | Cookie-based | HIGH |
| `spotify_api.rs` | rspotify crate | ytmusicapi-rs or HTTP | HIGH |
| `spotify_worker.rs` | librespot playback | yt-dlp + audio player | HIGH |
| `spotify.rs` | librespot wrapper | YouTube wrapper | HIGH |
| `spotify_url.rs` | Spotify URIs | YouTube URLs/IDs | MEDIUM |
| `model/*.rs` | Spotify models | YouTube Music models | MEDIUM |

### Components to Keep (mostly unchanged)

- `queue.rs` - Generic playback queue
- `library.rs` - Works with `ListItem` trait
- `commands.rs` - Generic command handling
- `ui/*.rs` - Uses `ListItem` trait, mostly generic

## Target Architecture

**Direct replacement approach** - no abstraction layer, just swap Spotify with YouTube Music.

### File Mapping (Spotify → YouTube Music)

| Old File | New File | Purpose |
|----------|----------|---------|
| `spotify.rs` | `youtube.rs` | High-level controller |
| `spotify_api.rs` | `youtube_music/client.rs` | API client |
| `spotify_worker.rs` | `player_worker.rs` | Background playback |
| `spotify_url.rs` | `youtube_url.rs` | URL parsing |
| `authentication.rs` | `authentication.rs` | Cookie-based auth |

### New Module Structure

```
src/
├── youtube.rs              # High-level YouTube Music interface
├── youtube_url.rs          # URL parsing
├── youtube_music/
│   ├── mod.rs
│   ├── cookies.rs          # Cookie parsing
│   ├── auth.rs             # SAPISID hash generation
│   ├── client.rs           # HTTP client with auth
│   └── api/
│       ├── mod.rs
│       ├── search.rs       # Search endpoint
│       ├── library.rs      # Library endpoints
│       ├── playlist.rs     # Playlist details
│       ├── album.rs        # Album details
│       └── artist.rs       # Artist details
├── player.rs               # Audio player (rodio-based)
└── player_worker.rs        # Background playback thread
```

## YouTube Music API Notes

### Authentication
YouTube Music uses cookie-based auth. Required cookies:
- `SAPISID` - API session ID
- `HSID`, `SSID`, `APISID`, `SID` - Session cookies
- `__Secure-1PSID`, `__Secure-3PSID` - Secure session cookies

### API Endpoints (reverse-engineered)
Base URL: `https://music.youtube.com/youtubei/v1/`

Key endpoints:
- `browse` - Library, playlists, albums, artists
- `search` - Search functionality
- `get_queue` - Get playback queue
- `player` - Get playback info (including stream URLs)

### Data Mapping

| YouTube Music | ncytm Model |
|---------------|-------------|
| Song | Track |
| Album | Album |
| Artist | Artist |
| Playlist | Playlist |
| Video (music video) | Could be Track variant or separate |

## Implementation Plan Reference

See `IMPLEMENTATION_PLAN.md` for the detailed, phased implementation plan.

## Testing Strategy

### Unit Tests
- Test each module in isolation
- Mock external dependencies (API, audio)
- Use `#[cfg(test)]` modules

### Integration Tests
- Test full flows with mock servers
- Test authentication flow
- Test search → play flow

### Manual Testing
- Verify TUI interactions
- Test with real YouTube Music account
- Test audio playback

## File Locations

- Skill file: `.opencode/skill.md` (this file)
- Implementation plan: `IMPLEMENTATION_PLAN.md`
- Source code: `src/`
- Tests: `src/*/tests.rs` or `tests/`
- Documentation: `doc/`

## Post-Change Workflow

After making code changes, the AI should:

1. **Build and test** to verify changes work:
   ```bash
   cargo build && cargo test
   ```

2. **Stage and commit** with conventional commit format:
   ```bash
   git add -A
   git commit -m "<type>: <description>"
   ```
   
   This triggers husky pre-commit hooks which run:
   - Code formatting (`cargo fmt`)
   - Linting (`cargo clippy`)
   - Any other configured checks

3. If the commit fails due to formatting/linting issues, fix them and retry.

## Useful Commands

```bash
# Build
cargo build

# Run tests
cargo test

# Run with debug logging
cargo run -- -d debug.log

# Build release
cargo build --release

# Format code manually
cargo fmt

# Run clippy linter
cargo clippy

# Stage and commit (triggers hooks)
git add -A && git commit -m "feat: description"
```

## Dependencies to Add (planned)

```toml
# YouTube Music API
reqwest = { version = "0.12", features = ["json", "cookies"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Audio extraction (one of these)
rusty_ytdl = "0.7"  # Pure Rust yt-dlp alternative
# OR call yt-dlp via subprocess

# Audio playback (already have rodio as optional)
rodio = "0.17"  # Or use existing audio backends
```

## Links & References

- [ytmusicapi (Python)](https://github.com/sigma67/ytmusicapi) - Reference implementation
- [YouTube Music internal API docs](https://github.com/sigma67/ytmusicapi/wiki) - API documentation
- [rusty_ytdl](https://github.com/Mithronn/rusty_ytdl) - Rust yt-dlp alternative
- [ncspot (original)](https://github.com/hrkfdn/ncspot) - Original project
