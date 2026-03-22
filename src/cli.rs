use std::time::Duration;

use crate::authentication;
use crate::config::{self, Config, user_cache_directory, user_configuration_directory};

/// Print platform info like which platform directories will be used.
pub fn info() -> Result<(), String> {
    let user_configuration_directory = user_configuration_directory();
    let user_cache_directory = user_cache_directory();

    println!(
        "USER_CONFIGURATION_PATH {}",
        user_configuration_directory
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or("not found".into())
    );
    println!(
        "USER_CACHE_PATH {}",
        user_cache_directory
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or("not found".into())
    );

    #[cfg(unix)]
    {
        use crate::utils::user_runtime_directory;

        let user_runtime_directory = user_runtime_directory();
        println!(
            "USER_RUNTIME_PATH {}",
            user_runtime_directory
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or("not found".into())
        );
    }

    Ok(())
}

/// Handle the auth subcommand.
pub fn auth(
    browser: bool,
    use_system_profile: bool,
    browser_type: &str,
    check: bool,
    timeout_secs: u64,
) -> Result<(), String> {
    if browser {
        auth_browser(use_system_profile, browser_type, timeout_secs)
    } else if check {
        auth_check()
    } else {
        auth_status()
    }
}

/// Check if current cookies are valid.
fn auth_check() -> Result<(), String> {
    let config = Config::new(None);

    match authentication::authenticate(&config) {
        Ok(result) => {
            println!("Authentication valid!");
            if let Some(name) = result.account_name {
                println!("Logged in as: {}", name);
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Authentication check failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Show current authentication status.
fn auth_status() -> Result<(), String> {
    let cookies_path = config::config_path("cookies.txt");

    println!("Authentication status:");
    println!();

    if cookies_path.exists() {
        println!("  Cookies file: {} (exists)", cookies_path.display());

        // Try to validate
        let config = Config::new(None);
        match authentication::authenticate(&config) {
            Ok(result) => {
                println!("  Status: Valid");
                if let Some(name) = result.account_name {
                    println!("  Account: {}", name);
                }
            }
            Err(e) => {
                println!("  Status: Invalid ({})", e);
                #[cfg(feature = "browser_auth")]
                println!();
                #[cfg(feature = "browser_auth")]
                println!("Run `ncytm auth --browser` to re-authenticate.");
            }
        }
    } else {
        println!("  Cookies file: {} (not found)", cookies_path.display());
        println!("  Status: Not authenticated");
        println!();
        #[cfg(feature = "browser_auth")]
        println!("Run `ncytm auth --browser` to authenticate.");
        #[cfg(not(feature = "browser_auth"))]
        {
            println!("{}", authentication::get_cookie_instructions());
        }
    }

    Ok(())
}

/// Launch browser for authentication.
#[cfg(feature = "browser_auth")]
fn auth_browser(
    use_system_profile: bool,
    browser_type: &str,
    timeout_secs: u64,
) -> Result<(), String> {
    use crate::browser_auth;

    let cookies_path = config::config_path("cookies.txt");
    let timeout = Duration::from_secs(timeout_secs);

    // Create tokio runtime for async browser operations
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| format!("Failed to create async runtime: {}", e))?;

    let result = runtime.block_on(async {
        browser_auth::authenticate_via_browser(
            &cookies_path,
            timeout,
            use_system_profile,
            browser_type,
        )
        .await
    });

    match result {
        Ok(auth_result) => {
            println!();
            println!("✅ Authentication successful!");
            println!(
                "Saved {} cookies to {}",
                auth_result.cookie_count,
                auth_result.cookies_path.display()
            );
            println!();
            println!("You can now run ncytm to start the application.");
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Browser authentication failed: {}", e);
            Err(e.to_string())
        }
    }
}

#[cfg(not(feature = "browser_auth"))]
fn auth_browser(
    _use_system_profile: bool,
    _browser_type: &str,
    _timeout_secs: u64,
) -> Result<(), String> {
    eprintln!("Browser authentication is not available.");
    eprintln!("ncytm was compiled without the 'browser_auth' feature.");
    eprintln!();
    eprintln!("{}", authentication::get_cookie_instructions());
    Err("Browser authentication not available".to_string())
}
