//! Stub module for authentication.
//! This will be replaced with YouTube Music cookie-based authentication.

use log::info;

use crate::config::Config;
use crate::spotify::Credentials;

/// Find a free port for potential OAuth callback (kept for compatibility).
pub fn find_free_port() -> Result<u16, String> {
    use std::net::TcpListener;
    let socket = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    socket
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|e| e.to_string())
}

/// Get credentials for authentication.
/// Currently stubbed - will be replaced with cookie-based auth for YouTube Music.
pub fn get_credentials(_configuration: &Config) -> Result<Credentials, String> {
    info!("Authentication stubbed - will be replaced with YouTube Music cookie auth");
    Ok(Credentials::default())
}

/// Stub for rspotify token refresh.
/// This will be removed when YouTube Music auth is implemented.
pub fn get_rspotify_token() -> Result<(), String> {
    info!("Token refresh stubbed");
    Ok(())
}
