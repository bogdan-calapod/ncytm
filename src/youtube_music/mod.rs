//! YouTube Music API client and authentication.
//!
//! This module provides the core functionality for interacting with YouTube Music:
//! - Cookie-based authentication
//! - API client for making requests
//! - Response parsing
//! - Audio stream extraction

pub mod api;
pub mod auth;
pub mod client;
pub mod cookies;
pub mod stream;

pub use client::{ClientError, YouTubeMusicClient};
pub use cookies::{CookieError, Cookies};
pub use stream::{AudioQuality, get_stream_url};
