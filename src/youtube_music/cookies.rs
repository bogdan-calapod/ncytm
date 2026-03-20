//! Cookie parser for YouTube Music authentication.
//!
//! Parses cookies from Netscape format cookie files (as exported by browser extensions).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use thiserror::Error;

/// Required cookies for YouTube Music API authentication.
const REQUIRED_COOKIES: &[&str] = &["SAPISID", "__Secure-1PSID", "__Secure-3PSID"];

/// Cookies to include in API requests. We filter out tracking cookies (ST-*)
/// which can make requests too large (413 errors).
const AUTH_COOKIES: &[&str] = &[
    "SAPISID",
    "__Secure-1PAPISID",
    "__Secure-3PAPISID",
    "APISID",
    "HSID",
    "SSID",
    "SID",
    "__Secure-1PSID",
    "__Secure-3PSID",
    "__Secure-1PSIDTS",
    "__Secure-3PSIDTS",
    "SIDCC",
    "__Secure-1PSIDCC",
    "__Secure-3PSIDCC",
    "LOGIN_INFO",
];

/// Errors that can occur when parsing cookies.
#[derive(Debug, Error)]
pub enum CookieError {
    #[error("Failed to read cookie file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Missing required cookie: {0}")]
    MissingCookie(String),

    #[error("Malformed cookie line: {0}")]
    MalformedLine(String),
}

/// A collection of cookies parsed from a Netscape format file.
#[derive(Debug, Clone)]
pub struct Cookies {
    jar: HashMap<String, String>,
}

impl Cookies {
    /// Parse cookies from a Netscape format cookie file.
    ///
    /// The Netscape cookie format has tab-separated fields:
    /// `domain  flag  path  secure  expiration  name  value`
    ///
    /// Lines starting with `#` are comments and are ignored.
    pub fn from_file(path: &Path) -> Result<Self, CookieError> {
        let content = fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// Parse cookies from a string in Netscape format.
    pub fn parse(content: &str) -> Result<Self, CookieError> {
        let mut jar = HashMap::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Split by tabs
            let fields: Vec<&str> = line.split('\t').collect();

            // Netscape format has 7 fields: domain, flag, path, secure, expiration, name, value
            if fields.len() < 7 {
                // Some files may have fewer fields, try to be lenient
                if fields.len() >= 6 {
                    // Might be missing the value (empty cookie)
                    let name = fields[5].to_string();
                    let value = fields.get(6).unwrap_or(&"").to_string();
                    jar.insert(name, value);
                    continue;
                }
                return Err(CookieError::MalformedLine(line.to_string()));
            }

            let name = fields[5].to_string();
            let value = fields[6].to_string();

            jar.insert(name, value);
        }

        Ok(Self { jar })
    }

    /// Get a cookie value by name.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.jar.get(name).map(|s| s.as_str())
    }

    /// Check if all required cookies for YouTube Music API are present.
    pub fn has_required(&self) -> bool {
        REQUIRED_COOKIES
            .iter()
            .all(|name| self.jar.get(*name).is_some_and(|v| !v.is_empty()))
    }

    /// Get a list of missing required cookies.
    pub fn missing_cookies(&self) -> Vec<&'static str> {
        REQUIRED_COOKIES
            .iter()
            .filter(|name| !self.jar.get(**name).is_some_and(|v| !v.is_empty()))
            .copied()
            .collect()
    }

    /// Validate that all required cookies are present.
    pub fn validate(&self) -> Result<(), CookieError> {
        for name in REQUIRED_COOKIES {
            if !self.jar.get(*name).is_some_and(|v| !v.is_empty()) {
                return Err(CookieError::MissingCookie(name.to_string()));
            }
        }
        Ok(())
    }

    /// Get the SAPISID cookie, required for API authentication.
    pub fn sapisid(&self) -> Option<&str> {
        self.get("SAPISID")
    }

    /// Get all cookies as a HashMap reference.
    pub fn all(&self) -> &HashMap<String, String> {
        &self.jar
    }

    /// Get only the authentication cookies needed for API requests.
    /// This filters out tracking cookies (ST-*, etc.) that can make requests too large.
    pub fn auth_cookies(&self) -> HashMap<String, String> {
        self.jar
            .iter()
            .filter(|(name, _)| AUTH_COOKIES.contains(&name.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Get the number of cookies.
    pub fn len(&self) -> usize {
        self.jar.len()
    }

    /// Check if the cookie jar is empty.
    pub fn is_empty(&self) -> bool {
        self.jar.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_COOKIE_FILE: &str = r#"# Netscape HTTP Cookie File
# https://curl.haxx.se/rfc/cookie_spec.html
# This is a generated file! Do not edit.

.youtube.com	TRUE	/	TRUE	1735689600	SAPISID	abc123xyz/ABCDEFGHIJ
.youtube.com	TRUE	/	TRUE	1735689600	APISID	def456uvw
.youtube.com	TRUE	/	TRUE	1735689600	HSID	ghi789rst
.youtube.com	TRUE	/	TRUE	1735689600	SID	jkl012mno
.youtube.com	TRUE	/	TRUE	1735689600	SSID	pqr345stu
.youtube.com	TRUE	/	TRUE	1735689600	__Secure-1PSID	vwx678abc
.youtube.com	TRUE	/	TRUE	1735689600	__Secure-3PSID	yza901bcd
"#;

    const MISSING_SAPISID: &str = r#"# Netscape HTTP Cookie File
.youtube.com	TRUE	/	TRUE	1735689600	__Secure-1PSID	vwx678abc
.youtube.com	TRUE	/	TRUE	1735689600	__Secure-3PSID	yza901bcd
"#;

    const MALFORMED_LINE: &str = r#"# Netscape HTTP Cookie File
.youtube.com	TRUE	/	TRUE
"#;

    #[test]
    fn test_parse_valid_cookie_file() {
        let cookies = Cookies::parse(VALID_COOKIE_FILE).unwrap();

        assert_eq!(cookies.len(), 7);
        assert_eq!(cookies.get("SAPISID"), Some("abc123xyz/ABCDEFGHIJ"));
        assert_eq!(cookies.get("APISID"), Some("def456uvw"));
        assert_eq!(cookies.get("__Secure-1PSID"), Some("vwx678abc"));
        assert_eq!(cookies.get("__Secure-3PSID"), Some("yza901bcd"));
    }

    #[test]
    fn test_has_required_cookies() {
        let cookies = Cookies::parse(VALID_COOKIE_FILE).unwrap();
        assert!(cookies.has_required());
    }

    #[test]
    fn test_missing_required_cookies() {
        let cookies = Cookies::parse(MISSING_SAPISID).unwrap();
        assert!(!cookies.has_required());

        let missing = cookies.missing_cookies();
        assert!(missing.contains(&"SAPISID"));
    }

    #[test]
    fn test_validate_returns_error_on_missing() {
        let cookies = Cookies::parse(MISSING_SAPISID).unwrap();
        let result = cookies.validate();
        assert!(result.is_err());

        if let Err(CookieError::MissingCookie(name)) = result {
            assert_eq!(name, "SAPISID");
        } else {
            panic!("Expected MissingCookie error");
        }
    }

    #[test]
    fn test_malformed_line_error() {
        let result = Cookies::parse(MALFORMED_LINE);
        assert!(result.is_err());

        if let Err(CookieError::MalformedLine(_)) = result {
            // Expected
        } else {
            panic!("Expected MalformedLine error");
        }
    }

    #[test]
    fn test_empty_file() {
        let cookies = Cookies::parse("").unwrap();
        assert!(cookies.is_empty());
        assert!(!cookies.has_required());
    }

    #[test]
    fn test_comments_only() {
        let content = "# This is a comment\n# Another comment\n";
        let cookies = Cookies::parse(content).unwrap();
        assert!(cookies.is_empty());
    }

    #[test]
    fn test_sapisid_accessor() {
        let cookies = Cookies::parse(VALID_COOKIE_FILE).unwrap();
        assert_eq!(cookies.sapisid(), Some("abc123xyz/ABCDEFGHIJ"));
    }

    #[test]
    fn test_get_nonexistent_cookie() {
        let cookies = Cookies::parse(VALID_COOKIE_FILE).unwrap();
        assert_eq!(cookies.get("NONEXISTENT"), None);
    }
}
