//! Browser-based authentication for YouTube Music.
//!
//! This module provides functionality to launch a browser window for the user to
//! log in to YouTube Music, then extracts and saves the authentication cookies.
//!
//! Uses a persistent user data directory to maintain browser state across sessions,
//! which helps avoid Google's automated browser detection.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use thiserror::Error;

/// The URL to navigate to for YouTube Music login.
const YOUTUBE_MUSIC_URL: &str = "https://music.youtube.com";

/// Directory name for storing browser profile data.
const BROWSER_PROFILE_DIR: &str = "browser_profile";

/// Cookies that indicate successful authentication.
const AUTH_INDICATOR_COOKIES: &[&str] = &["SAPISID", "__Secure-1PSID", "__Secure-3PSID"];

/// Errors that can occur during browser-based authentication.
#[derive(Debug, Error)]
pub enum BrowserAuthError {
    #[error("Failed to launch browser: {0}")]
    BrowserLaunch(String),

    #[error("Failed to extract cookies: {0}")]
    CookieExtraction(String),

    #[error("Failed to save cookies: {0}")]
    CookieSave(#[from] std::io::Error),

    #[error("Authentication timeout - no valid cookies found")]
    Timeout,
}

/// Result of browser-based authentication.
pub struct BrowserAuthResult {
    /// Number of cookies extracted.
    pub cookie_count: usize,
    /// Path where cookies were saved.
    pub cookies_path: std::path::PathBuf,
}

/// Get the path to the browser profile directory.
///
/// This creates a persistent directory for the browser profile, which helps
/// avoid Google's automated browser detection by maintaining browser state.
fn get_ncytm_profile_dir(cookies_path: &Path) -> PathBuf {
    // Store browser profile alongside the cookies file
    cookies_path
        .parent()
        .map(|p| p.join(BROWSER_PROFILE_DIR))
        .unwrap_or_else(|| PathBuf::from(BROWSER_PROFILE_DIR))
}

/// Detect the system browser profile directory for the specified browser type.
///
/// Returns the path to the user's browser profile if found.
fn detect_system_browser_profile(browser_type: &str) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").ok()?;
        let path = match browser_type {
            "chrome" => format!("{}/Library/Application Support/Google/Chrome", home),
            "chromium" => format!("{}/Library/Application Support/Chromium", home),
            "edge" => format!("{}/Library/Application Support/Microsoft Edge", home),
            _ => return None,
        };
        let p = PathBuf::from(&path);
        if p.exists() {
            return Some(p);
        }
    }

    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").ok()?;
        let path = match browser_type {
            "chrome" => format!("{}/.config/google-chrome", home),
            "chromium" => format!("{}/.config/chromium", home),
            "edge" => format!("{}/.config/microsoft-edge", home),
            _ => return None,
        };
        let p = PathBuf::from(&path);
        if p.exists() {
            return Some(p);
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let path = match browser_type {
                "chrome" => format!(r"{}\Google\Chrome\User Data", local_app_data),
                "chromium" => format!(r"{}\Chromium\User Data", local_app_data),
                "edge" => format!(r"{}\Microsoft\Edge\User Data", local_app_data),
                _ => return None,
            };
            let p = PathBuf::from(&path);
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

/// Get the browser executable path for the specified browser type.
fn get_browser_executable(browser_type: &str) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let paths = match browser_type {
            "chrome" => vec!["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"],
            "chromium" => vec!["/Applications/Chromium.app/Contents/MacOS/Chromium"],
            "edge" => vec!["/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"],
            _ => return None,
        };
        for path in paths {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some(p);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let paths = match browser_type {
            "chrome" => vec!["google-chrome", "google-chrome-stable"],
            "chromium" => vec!["chromium", "chromium-browser"],
            "edge" => vec!["microsoft-edge", "microsoft-edge-stable"],
            _ => return None,
        };
        for name in paths {
            if let Ok(output) = std::process::Command::new("which").arg(name).output() {
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !path.is_empty() {
                        return Some(PathBuf::from(path));
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let program_files = std::env::var("ProgramFiles").unwrap_or_default();
        let program_files_x86 = std::env::var("ProgramFiles(x86)").unwrap_or_default();

        let paths = match browser_type {
            "chrome" => vec![
                format!(r"{}\Google\Chrome\Application\chrome.exe", program_files),
                format!(
                    r"{}\Google\Chrome\Application\chrome.exe",
                    program_files_x86
                ),
            ],
            "chromium" => vec![format!(
                r"{}\Chromium\Application\chrome.exe",
                program_files
            )],
            "edge" => vec![
                format!(r"{}\Microsoft\Edge\Application\msedge.exe", program_files),
                format!(
                    r"{}\Microsoft\Edge\Application\msedge.exe",
                    program_files_x86
                ),
            ],
            _ => return None,
        };
        for path in paths {
            let p = PathBuf::from(&path);
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

/// Launch a browser for the user to authenticate with YouTube Music.
///
/// This function:
/// 1. Launches a visible browser with a persistent profile
/// 2. Navigates to YouTube Music
/// 3. Waits for the user to log in (polls for auth cookies)
/// 4. Extracts all cookies and saves them in Netscape format
///
/// The browser uses a persistent user data directory to maintain state across
/// sessions. This helps avoid Google's "browser not secure" warnings by making
/// the browser appear more like a regular user browser.
///
/// # Arguments
///
/// * `cookies_path` - Path where cookies should be saved
/// * `timeout` - Maximum time to wait for authentication
/// * `use_system_profile` - If true, use the system browser profile
/// * `browser_type` - Which browser to use: "chrome", "edge", or "chromium"
///
/// # Returns
///
/// Result containing information about the saved cookies.
pub async fn authenticate_via_browser(
    cookies_path: &Path,
    timeout: Duration,
    use_system_profile: bool,
    browser_type: &str,
) -> Result<BrowserAuthResult, BrowserAuthError> {
    // Find the browser executable
    let browser_exe = get_browser_executable(browser_type).ok_or_else(|| {
        BrowserAuthError::BrowserLaunch(format!(
            "Could not find {} browser. Make sure it's installed.",
            browser_type
        ))
    })?;

    println!("Using browser: {}", browser_exe.display());

    // Get browser profile directory
    let profile_dir = if use_system_profile {
        match detect_system_browser_profile(browser_type) {
            Some(path) => {
                println!("Using system {} profile: {}", browser_type, path.display());
                println!();
                println!(
                    "NOTE: Please close {} if it's currently running,",
                    browser_type
                );
                println!("      as only one instance can use a profile at a time.");
                println!();
                path
            }
            None => {
                return Err(BrowserAuthError::BrowserLaunch(format!(
                    "Could not find system {} profile. \
                     Try running without --use-system-profile.",
                    browser_type
                )));
            }
        }
    } else {
        let dir = get_ncytm_profile_dir(cookies_path);
        // Ensure profile directory exists
        std::fs::create_dir_all(&dir).map_err(|e| {
            BrowserAuthError::BrowserLaunch(format!(
                "Failed to create browser profile directory: {}",
                e
            ))
        })?;
        dir
    };

    println!("Launching browser for YouTube Music authentication...");
    if !use_system_profile {
        println!("Browser profile: {}", profile_dir.display());
    }
    println!();
    println!("Please log in to your YouTube Music account in the browser window.");
    println!("The browser will close automatically once authentication is detected.");
    println!();

    // Configure browser to be visible (not headless) with persistent profile
    // and anti-detection flags to bypass Google's automation detection
    let config = BrowserConfig::builder()
        .chrome_executable(&browser_exe) // Use specified browser
        .with_head() // Visible browser
        .viewport(None) // Use default viewport
        .user_data_dir(&profile_dir) // Persistent profile directory
        // Anti-automation detection flags
        .arg("--disable-blink-features=AutomationControlled")
        .arg("--disable-infobars")
        .arg("--disable-extensions")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        // Disable "Chrome is being controlled by automated test software" banner
        .arg("--disable-background-networking")
        .arg("--disable-client-side-phishing-detection")
        .arg("--disable-default-apps")
        .arg("--disable-hang-monitor")
        .arg("--disable-popup-blocking")
        .arg("--disable-prompt-on-repost")
        .arg("--disable-sync")
        .arg("--disable-translate")
        .arg("--metrics-recording-only")
        .arg("--safebrowsing-disable-auto-update")
        // Make it look more like a regular browser
        .arg("--window-size=1280,900")
        .build()
        .map_err(|e| BrowserAuthError::BrowserLaunch(e.to_string()))?;

    // Launch browser
    let (browser, mut handler) = Browser::launch(config)
        .await
        .map_err(|e| BrowserAuthError::BrowserLaunch(e.to_string()))?;

    // Spawn handler to process browser events
    let handle = tokio::spawn(async move {
        while let Some(_event) = handler.next().await {
            // Process events to keep browser responsive
        }
    });

    // Create a new page and navigate to YouTube Music
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| BrowserAuthError::BrowserLaunch(e.to_string()))?;

    // Inject JavaScript to remove automation detection traces BEFORE navigating
    // This helps bypass Google's navigator.webdriver check
    let _ = page
        .evaluate(
            r#"
        Object.defineProperty(navigator, 'webdriver', {
            get: () => undefined
        });
    "#,
        )
        .await;

    // Now navigate to YouTube Music
    page.goto(YOUTUBE_MUSIC_URL).await.map_err(|e| {
        BrowserAuthError::BrowserLaunch(format!("Failed to navigate to YouTube Music: {}", e))
    })?;

    // Wait a moment for the page to start loading
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Inject more anti-detection JavaScript after page load
    let _ = page
        .evaluate(
            r#"
        // Remove automation-related properties
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Array;
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Promise;
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Symbol;
        
        // Override permissions query
        const originalQuery = window.navigator.permissions.query;
        window.navigator.permissions.query = (parameters) => (
            parameters.name === 'notifications' ?
                Promise.resolve({ state: Notification.permission }) :
                originalQuery(parameters)
        );
    "#,
        )
        .await;

    // Poll for authentication cookies
    let start = std::time::Instant::now();
    let poll_interval = Duration::from_secs(2);

    let cookies = loop {
        if start.elapsed() > timeout {
            // Clean up
            drop(browser);
            handle.abort();
            return Err(BrowserAuthError::Timeout);
        }

        // Get current cookies
        let current_cookies = page
            .get_cookies()
            .await
            .map_err(|e| BrowserAuthError::CookieExtraction(e.to_string()))?;

        // Check if we have the required auth cookies
        let has_auth = AUTH_INDICATOR_COOKIES.iter().all(|name| {
            current_cookies
                .iter()
                .any(|c| c.name == *name && !c.value.is_empty())
        });

        if has_auth {
            println!("Authentication detected! Extracting cookies...");
            break current_cookies;
        }

        // Wait before polling again
        tokio::time::sleep(poll_interval).await;
    };

    // Convert cookies to Netscape format and save
    let netscape_content = cookies_to_netscape(&cookies);
    let cookie_count = cookies.len();

    // Ensure parent directory exists
    if let Some(parent) = cookies_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(cookies_path, &netscape_content)?;

    // Clean up browser
    drop(browser);
    handle.abort();

    println!("Cookies saved to: {}", cookies_path.display());

    Ok(BrowserAuthResult {
        cookie_count,
        cookies_path: cookies_path.to_path_buf(),
    })
}

/// Convert cookies to Netscape cookie file format.
///
/// Format: `domain\tflag\tpath\tsecure\texpiration\tname\tvalue`
fn cookies_to_netscape(
    cookies: &[chromiumoxide::cdp::browser_protocol::network::Cookie],
) -> String {
    let mut lines = vec![
        "# Netscape HTTP Cookie File".to_string(),
        "# https://curl.haxx.se/rfc/cookie_spec.html".to_string(),
        "# Generated by ncytm browser authentication".to_string(),
        String::new(),
    ];

    for cookie in cookies {
        // Filter to only YouTube-related cookies
        if !cookie.domain.contains("youtube") && !cookie.domain.contains("google") {
            continue;
        }

        let domain = if cookie.domain.starts_with('.') {
            cookie.domain.clone()
        } else {
            format!(".{}", cookie.domain)
        };

        let flag = "TRUE"; // Host-only flag
        let path = &cookie.path;
        let secure = if cookie.secure { "TRUE" } else { "FALSE" };

        // Expiration: use the cookie's expiration or a far future date
        // expires is -1 for session cookies
        let expiration = if cookie.expires < 0.0 {
            253402300799 // Year 9999 for session cookies
        } else {
            cookie.expires as i64
        };

        let line = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            domain, flag, path, secure, expiration, cookie.name, cookie.value
        );
        lines.push(line);
    }

    lines.join("\n")
}

// Tests for browser_auth module
// Note: Integration tests require a browser and are not included here.
// The cookies_to_netscape function is tested implicitly through actual browser auth.
