# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Fork of ncspot to create ncytm (YouTube Music client)
- Updated documentation and branding for ncytm

### Changed

- Renamed project from ncspot to ncytm
- Updated all documentation to reflect YouTube Music focus

---

## Previous ncspot Changelog

*The changelog below documents changes from the original ncspot project prior to this fork.*

## [1.3.3] - ncspot

### Fixed

- Fix crashing when attempting to add a song to a playlist
- Fix incorrect shuffle order after appending a track while shuffle is enabled
- Fix token generation issues after Spotify API changes

### Added

- **Added new Vim motions** for moving to the top/bottom of a page (`g` and `G`)

## [1.3.2] - ncspot

### Fixed

- Playlist retrieval crashing when list contains podcast episodes
- Crash when shifting a song by an amount greater than the queue's length
- Crash when displaying songs that do not have an (available) artist
- Playback broken due to Spotify API change

## [1.3.1] - ncspot

### Fixed

- Bug preventing any type of playback due to spotify API changes.
- Bug preventing retrieval of new song metadata from spotify.

## [1.3.0] - ncspot

### Added

- Automatically find free port for OAuth2 login flow

### Fixed

- Skip unplayable tracks
- Queue UI correctly plays a track when clicking on an already selected item

*For full ncspot changelog history, see [ncspot releases](https://github.com/hrkfdn/ncspot/releases)*
