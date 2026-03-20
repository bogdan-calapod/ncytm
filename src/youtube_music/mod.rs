//! YouTube Music API client and authentication.
//!
//! This module provides the core functionality for interacting with YouTube Music:
//! - Cookie-based authentication
//! - API client for making requests
//! - Response parsing

pub mod auth;
pub mod client;
pub mod cookies;

pub use auth::{generate_sapisid_hash, YOUTUBE_MUSIC_ORIGIN};
pub use client::{AccountInfo, ClientError, YouTubeMusicClient};
pub use cookies::{CookieError, Cookies};
