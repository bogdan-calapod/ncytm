//! Cookie-based authentication for YouTube Music.
//!
//! YouTube Music uses cookie-based authentication. Users need to export their
//! cookies from a browser session where they're logged into YouTube Music.

use std::path::{Path, PathBuf};

use log::{debug, error, info, warn};
use tokio::runtime::Runtime;

use crate::config::{self, Config};
use crate::youtube_music::{CookieError, Cookies, YouTubeMusicClient};

/// Errors that can occur during authentication.
#[derive(Debug)]
pub enum AuthError {
    /// Cookies file not found.
    CookiesNotFound(PathBuf),
    /// Failed to parse cookies file.
    CookieParseError(CookieError),
    /// Cookies are missing required values.
    MissingCookies(String),
    /// Cookies are invalid or expired.
    InvalidCookies(String),
    /// Failed to verify cookies with API.
    VerificationFailed(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CookiesNotFound(path) => write!(f, "Cookies file not found: {}", path.display()),
            Self::CookieParseError(e) => write!(f, "Failed to parse cookies: {}", e),
            Self::MissingCookies(msg) => write!(f, "Missing required cookies: {}", msg),
            Self::InvalidCookies(msg) => write!(f, "Invalid or expired cookies: {}", msg),
            Self::VerificationFailed(msg) => write!(f, "Cookie verification failed: {}", msg),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<CookieError> for AuthError {
    fn from(e: CookieError) -> Self {
        Self::CookieParseError(e)
    }
}

/// Result of authentication.
#[derive(Debug, Clone)]
pub struct AuthResult {
    /// The loaded cookies.
    pub cookies: Cookies,
    /// Account name (if available).
    pub account_name: Option<String>,
    /// Channel handle (if available).
    pub channel_handle: Option<String>,
}

/// Authenticate with YouTube Music using cookies.
///
/// This function:
/// 1. Looks for cookies file at the configured path
/// 2. Parses and validates the cookies
/// 3. Verifies the cookies work by making an API call
///
/// # Arguments
///
/// * `config` - Application configuration
///
/// # Returns
///
/// Authentication result with cookies and account info, or an error.
pub fn authenticate(config: &Config) -> Result<AuthResult, AuthError> {
    info!("Authenticating with YouTube Music");

    // Get cookies file path
    let cookies_path = get_cookies_path(config);
    debug!("Looking for cookies at: {}", cookies_path.display());

    // Check if file exists
    if !cookies_path.exists() {
        return Err(AuthError::CookiesNotFound(cookies_path));
    }

    // Load and parse cookies
    let cookies = Cookies::from_file(&cookies_path)?;
    debug!("Loaded cookies from file");

    // Validate required cookies are present
    if !cookies.has_required() {
        return Err(AuthError::MissingCookies(
            "Missing SAPISID or other required cookies".to_string(),
        ));
    }
    debug!("Required cookies present");

    // Verify cookies work by making an API call
    let auth_result = verify_cookies(cookies)?;

    info!(
        "Authentication successful: {}",
        auth_result
            .account_name
            .as_deref()
            .unwrap_or("Unknown account")
    );

    Ok(auth_result)
}

/// Get the path to the cookies file.
fn get_cookies_path(config: &Config) -> PathBuf {
    // Check config for custom cookies path
    if let Some(ref cookies_file) = config.values().cookies_file {
        // If it's an absolute path, use it directly
        let path = PathBuf::from(cookies_file);
        if path.is_absolute() {
            return path;
        }
        // Otherwise, treat as relative to config directory
        return config::config_path(cookies_file);
    }

    // Default: look for cookies.txt in config directory
    config::config_path("cookies.txt")
}

/// Verify that the cookies are valid by making an API call.
fn verify_cookies(cookies: Cookies) -> Result<AuthResult, AuthError> {
    // Create a tokio runtime for the async call
    let runtime = Runtime::new()
        .map_err(|e| AuthError::VerificationFailed(format!("Failed to create runtime: {}", e)))?;

    // Create client
    let client = YouTubeMusicClient::new(cookies.clone())
        .map_err(|e| AuthError::InvalidCookies(format!("Failed to create client: {}", e)))?;

    // Verify auth
    let account_info = runtime
        .block_on(async { client.verify_auth().await })
        .map_err(|e| AuthError::InvalidCookies(format!("Authentication failed: {}", e)))?;

    Ok(AuthResult {
        cookies,
        account_name: account_info.name,
        channel_handle: account_info.channel_handle,
    })
}

/// Check if valid cookies exist without fully authenticating.
pub fn has_valid_cookies(config: &Config) -> bool {
    let cookies_path = get_cookies_path(config);

    if !cookies_path.exists() {
        return false;
    }

    match Cookies::from_file(&cookies_path) {
        Ok(cookies) => cookies.has_required(),
        Err(_) => false,
    }
}

/// Get instructions for obtaining cookies.
pub fn get_cookie_instructions() -> &'static str {
    r#"
To use ncytm, you need to export your YouTube Music cookies:

1. Install a browser extension to export cookies in Netscape format:
   - Firefox: "cookies.txt" by Lennon Hill
   - Chrome: "Get cookies.txt LOCALLY" or similar

2. Log in to music.youtube.com in your browser

3. Use the extension to export cookies for music.youtube.com

4. Save the exported file as:
   ~/.config/ncytm/cookies.txt

5. Restart ncytm

Required cookies: SAPISID, HSID, SSID, SID, __Secure-1PSID, __Secure-3PSID

Note: Cookies may expire after some time (usually weeks to months).
If playback stops working, export fresh cookies.
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_error_display() {
        let err = AuthError::CookiesNotFound(PathBuf::from("/test/path"));
        assert!(err.to_string().contains("/test/path"));

        let err = AuthError::MissingCookies("SAPISID".to_string());
        assert!(err.to_string().contains("SAPISID"));
    }

    #[test]
    fn test_get_cookie_instructions() {
        let instructions = get_cookie_instructions();
        assert!(instructions.contains("SAPISID"));
        assert!(instructions.contains("cookies.txt"));
    }
}
