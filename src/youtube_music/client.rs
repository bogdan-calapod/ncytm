//! YouTube Music API HTTP client.
//!
//! Provides an authenticated HTTP client for making requests to the YouTube Music API.

use std::collections::HashMap;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, COOKIE, ORIGIN, REFERER, USER_AGENT};
use serde_json::{Value, json};
use thiserror::Error;

use super::auth::{generate_sapisid_hash, YOUTUBE_MUSIC_ORIGIN};
use super::cookies::Cookies;

/// YouTube Music API base URL.
pub const API_BASE_URL: &str = "https://music.youtube.com/youtubei/v1";

/// YouTube Music API key (public, embedded in the web client).
pub const API_KEY: &str = "AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30";

/// User agent for API requests.
const USER_AGENT_VALUE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Errors that can occur when making API requests.
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Missing SAPISID cookie")]
    MissingSapisid,

    #[error("API error: {message}")]
    ApiError { message: String },

    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Invalid response format")]
    InvalidResponse,
}

/// YouTube Music API client with cookie-based authentication.
#[derive(Clone)]
pub struct YouTubeMusicClient {
    http: reqwest::Client,
    cookies: Cookies,
}

impl YouTubeMusicClient {
    /// Create a new YouTube Music client with the given cookies.
    pub fn new(cookies: Cookies) -> Result<Self, ClientError> {
        // Verify SAPISID is present
        if cookies.sapisid().is_none() {
            return Err(ClientError::MissingSapisid);
        }

        let http = reqwest::Client::builder()
            .cookie_store(true)
            .build()?;

        Ok(Self { http, cookies })
    }

    /// Build the authentication headers for a request.
    fn build_headers(&self) -> Result<HeaderMap, ClientError> {
        let mut headers = HeaderMap::new();

        // User agent
        headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));

        // Origin and referer
        headers.insert(ORIGIN, HeaderValue::from_static(YOUTUBE_MUSIC_ORIGIN));
        headers.insert(REFERER, HeaderValue::from_str(&format!("{}/", YOUTUBE_MUSIC_ORIGIN)).unwrap());

        // Content type
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        // SAPISID hash authorization
        let sapisid = self.cookies.sapisid().ok_or(ClientError::MissingSapisid)?;
        let auth_hash = generate_sapisid_hash(sapisid, YOUTUBE_MUSIC_ORIGIN);
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth_hash).unwrap());

        // Build cookie header
        let cookie_str = self.build_cookie_string();
        headers.insert(COOKIE, HeaderValue::from_str(&cookie_str).unwrap());

        // Additional YouTube-specific headers
        headers.insert(
            "X-Youtube-Client-Name",
            HeaderValue::from_static("67"),
        );
        headers.insert(
            "X-Youtube-Client-Version",
            HeaderValue::from_static("1.20231215.01.00"),
        );

        Ok(headers)
    }

    /// Build the cookie string for the Cookie header.
    fn build_cookie_string(&self) -> String {
        self.cookies
            .all()
            .iter()
            .map(|(name, value)| format!("{}={}", name, value))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Build the context object for API requests.
    fn build_context() -> Value {
        json!({
            "client": {
                "clientName": "WEB_REMIX",
                "clientVersion": "1.20231215.01.00",
                "hl": "en",
                "gl": "US",
                "experimentIds": [],
                "experimentsToken": "",
                "browserName": "Chrome",
                "browserVersion": "120.0.0.0",
                "osName": "Windows",
                "osVersion": "10.0",
                "platform": "DESKTOP",
                "userInterfaceTheme": "USER_INTERFACE_THEME_DARK"
            },
            "user": {
                "lockedSafetyMode": false
            },
            "request": {
                "useSsl": true,
                "internalExperimentFlags": []
            }
        })
    }

    /// Make a POST request to a YouTube Music API endpoint.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - The API endpoint (e.g., "browse", "search")
    /// * `body` - Additional body parameters to merge with the context
    ///
    /// # Returns
    ///
    /// The JSON response from the API.
    pub async fn post(&self, endpoint: &str, body: &Value) -> Result<Value, ClientError> {
        let url = format!("{}/{}?key={}&prettyPrint=false", API_BASE_URL, endpoint, API_KEY);

        // Build request body with context
        let mut request_body = json!({
            "context": Self::build_context()
        });

        // Merge additional body parameters
        if let Value::Object(map) = body {
            if let Value::Object(ref mut req_map) = request_body {
                for (key, value) in map {
                    req_map.insert(key.clone(), value.clone());
                }
            }
        }

        let headers = self.build_headers()?;

        let response = self.http
            .post(&url)
            .headers(headers)
            .json(&request_body)
            .send()
            .await?;

        // Check for HTTP errors
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(ClientError::ApiError {
                message: format!("HTTP {}: {}", status, error_text),
            });
        }

        let json: Value = response.json().await?;
        Ok(json)
    }

    /// Get a reference to the cookies.
    pub fn cookies(&self) -> &Cookies {
        &self.cookies
    }

    /// Verify that the authentication cookies are valid.
    ///
    /// This makes a request to the account menu endpoint to verify the cookies work.
    /// Returns account information if successful.
    pub async fn verify_auth(&self) -> Result<AccountInfo, ClientError> {
        let response = self.post("account/account_menu", &json!({})).await?;

        // Try to extract account information from the response
        // The response structure contains the user's account details
        let account_name = response
            .pointer("/actions/0/openPopupAction/popup/multiPageMenuRenderer/header/activeAccountHeaderRenderer/accountName/runs/0/text")
            .and_then(|v| v.as_str())
            .map(String::from);

        let channel_handle = response
            .pointer("/actions/0/openPopupAction/popup/multiPageMenuRenderer/header/activeAccountHeaderRenderer/channelHandle/runs/0/text")
            .and_then(|v| v.as_str())
            .map(String::from);

        let account_photo_url = response
            .pointer("/actions/0/openPopupAction/popup/multiPageMenuRenderer/header/activeAccountHeaderRenderer/accountPhoto/thumbnails/0/url")
            .and_then(|v| v.as_str())
            .map(String::from);

        // If we got a valid response with account info, authentication is working
        if account_name.is_some() || channel_handle.is_some() {
            Ok(AccountInfo {
                name: account_name,
                channel_handle,
                photo_url: account_photo_url,
            })
        } else {
            // Check if there's an error in the response
            if let Some(error) = response.get("error") {
                let message = error
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error");
                Err(ClientError::ApiError {
                    message: message.to_string(),
                })
            } else {
                // No account info and no explicit error - cookies might be invalid
                Err(ClientError::ApiError {
                    message: "Could not verify authentication. Cookies may be invalid or expired.".to_string(),
                })
            }
        }
    }
}

/// Information about the authenticated user's account.
#[derive(Debug, Clone)]
pub struct AccountInfo {
    /// The display name of the account.
    pub name: Option<String>,
    /// The channel handle (e.g., "@username").
    pub channel_handle: Option<String>,
    /// URL to the account's profile photo.
    pub photo_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_cookies() -> Cookies {
        let content = r#"# Netscape HTTP Cookie File
.youtube.com	TRUE	/	TRUE	1735689600	SAPISID	abc123xyz/ABCDEFGHIJ
.youtube.com	TRUE	/	TRUE	1735689600	APISID	def456uvw
.youtube.com	TRUE	/	TRUE	1735689600	HSID	ghi789rst
.youtube.com	TRUE	/	TRUE	1735689600	SID	jkl012mno
.youtube.com	TRUE	/	TRUE	1735689600	SSID	pqr345stu
.youtube.com	TRUE	/	TRUE	1735689600	__Secure-1PSID	vwx678abc
.youtube.com	TRUE	/	TRUE	1735689600	__Secure-3PSID	yza901bcd
"#;
        Cookies::parse(content).unwrap()
    }

    #[test]
    fn test_client_creation_with_valid_cookies() {
        let cookies = create_test_cookies();
        let client = YouTubeMusicClient::new(cookies);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_creation_without_sapisid() {
        let content = r#"# Netscape HTTP Cookie File
.youtube.com	TRUE	/	TRUE	1735689600	SID	jkl012mno
"#;
        let cookies = Cookies::parse(content).unwrap();
        let client = YouTubeMusicClient::new(cookies);
        assert!(matches!(client, Err(ClientError::MissingSapisid)));
    }

    #[test]
    fn test_build_headers_includes_auth() {
        let cookies = create_test_cookies();
        let client = YouTubeMusicClient::new(cookies).unwrap();
        let headers = client.build_headers().unwrap();

        // Check Authorization header exists and has correct format
        let auth = headers.get(AUTHORIZATION).unwrap().to_str().unwrap();
        assert!(auth.starts_with("SAPISIDHASH "));

        // Check other required headers
        assert!(headers.get(USER_AGENT).is_some());
        assert!(headers.get(ORIGIN).is_some());
        assert!(headers.get(REFERER).is_some());
        assert!(headers.get(CONTENT_TYPE).is_some());
        assert!(headers.get(COOKIE).is_some());
    }

    #[test]
    fn test_build_cookie_string() {
        let cookies = create_test_cookies();
        let client = YouTubeMusicClient::new(cookies).unwrap();
        let cookie_str = client.build_cookie_string();

        // Check that all cookies are in the string
        assert!(cookie_str.contains("SAPISID=abc123xyz/ABCDEFGHIJ"));
        assert!(cookie_str.contains("SID=jkl012mno"));
    }

    #[test]
    fn test_build_context_structure() {
        let context = YouTubeMusicClient::build_context();

        // Verify context structure
        assert!(context.get("client").is_some());
        assert!(context.get("user").is_some());
        assert!(context.get("request").is_some());

        let client = context.get("client").unwrap();
        assert_eq!(client.get("clientName").unwrap(), "WEB_REMIX");
        assert!(client.get("clientVersion").is_some());
    }

    #[test]
    fn test_account_info_structure() {
        // Test AccountInfo can be created and accessed
        let info = AccountInfo {
            name: Some("Test User".to_string()),
            channel_handle: Some("@testuser".to_string()),
            photo_url: Some("https://example.com/photo.jpg".to_string()),
        };

        assert_eq!(info.name, Some("Test User".to_string()));
        assert_eq!(info.channel_handle, Some("@testuser".to_string()));
        assert_eq!(info.photo_url, Some("https://example.com/photo.jpg".to_string()));
    }

    // Integration test for verify_auth - requires real cookies to run
    // Run with: cargo test --ignored -- test_verify_auth_integration
    #[tokio::test]
    #[ignore]
    async fn test_verify_auth_integration() {
        // This test requires actual cookies from ~/.config/ncytm/cookies.txt
        use std::path::PathBuf;
        
        let cookies_path = PathBuf::from(std::env::var("HOME").unwrap())
            .join(".config/ncytm/cookies.txt");
        
        if !cookies_path.exists() {
            eprintln!("Skipping integration test - no cookies file found at {:?}", cookies_path);
            return;
        }

        let cookies = Cookies::from_file(&cookies_path).expect("Failed to load cookies");
        let client = YouTubeMusicClient::new(cookies).expect("Failed to create client");
        
        match client.verify_auth().await {
            Ok(info) => {
                println!("Authentication successful!");
                println!("Account name: {:?}", info.name);
                println!("Channel handle: {:?}", info.channel_handle);
            }
            Err(e) => {
                panic!("Authentication failed: {:?}", e);
            }
        }
    }
}
