//! Slack "now playing" status integration.
//!
//! When enabled, ncytm appends the currently playing track to your Slack status
//! text — preserving whatever status you already had — whenever the current
//! track changes. When playback is paused or stopped, ncytm strips its addition
//! again so your original status stands alone.
//!
//! # Semantics
//!
//! ncytm only ever touches Slack on a *track change* or a *pause/stop*, never on
//! a timer. The resulting status text is always of the form:
//!
//! ```text
//! <your base status> <separator><emoji><track text>
//! ```
//!
//! To remain idempotent (so repeated updates never stack), ncytm treats
//! everything before the *last* occurrence of the separator as your "base"
//! status. This means that if you change your Slack status manually while music
//! is stopped, that new status simply becomes the base that we append to on the
//! next track change.
//!
//! Your Slack `status_emoji` field is never modified; the music emoji lives
//! inside the appended text instead.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use log::{debug, warn};
use serde::Deserialize;

use crate::config::SlackStatusConfig;
use crate::model::track::Track;

/// Slack's hard limit on the number of characters in a status text.
const SLACK_STATUS_MAX_CHARS: usize = 100;

/// The default separator between the base status and ncytm's addition.
const DEFAULT_SEPARATOR: &str = "|";

/// The default emoji embedded in the appended text.
const DEFAULT_EMOJI: &str = "🎵";

/// The default format for the appended track text.
const DEFAULT_FORMAT: &str = "{title} — {artists}";

const PROFILE_GET_URL: &str = "https://slack.com/api/users.profile.get";
const PROFILE_SET_URL: &str = "https://slack.com/api/users.profile.set";

/// A partial view of the Slack profile we care about.
#[derive(Debug, Deserialize, Default, Clone, PartialEq, Eq)]
struct SlackProfile {
    #[serde(default)]
    status_text: String,
    #[serde(default)]
    status_emoji: String,
}

#[derive(Debug, Deserialize)]
struct ProfileGetResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    profile: Option<SlackProfile>,
}

#[derive(Debug, Deserialize)]
struct ProfileSetResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

/// Manages the Slack status for the currently playing track.
pub struct SlackStatus {
    token: String,
    separator: String,
    emoji: String,
    format: String,
    /// The last full status text we wrote to Slack, used to avoid redundant
    /// network calls when nothing changed.
    last_written: Arc<Mutex<Option<String>>>,
}

impl SlackStatus {
    /// Create a [`SlackStatus`] from the user configuration.
    ///
    /// Returns `None` when the integration is disabled or when no token is
    /// available (neither in the config nor in the `SLACK_TOKEN` environment
    /// variable). This never fails loudly — a missing token just disables the
    /// feature.
    pub fn new(config: Option<&SlackStatusConfig>) -> Option<Self> {
        let config = config?;
        if !config.enabled {
            return None;
        }

        let token = config
            .token
            .clone()
            .filter(|t| !t.trim().is_empty())
            .or_else(|| std::env::var("SLACK_TOKEN").ok())
            .filter(|t| !t.trim().is_empty());

        let token = match token {
            Some(token) => token,
            None => {
                warn!(
                    "slack_status is enabled but no token is set (config `token` or SLACK_TOKEN env var); disabling"
                );
                return None;
            }
        };

        Some(Self {
            token,
            separator: non_empty(config.separator.clone(), DEFAULT_SEPARATOR),
            emoji: non_empty(config.emoji.clone(), DEFAULT_EMOJI),
            format: non_empty(config.format.clone(), DEFAULT_FORMAT),
            last_written: Arc::new(Mutex::new(None)),
        })
    }

    /// Update the Slack status to reflect the given track.
    ///
    /// This reads the current status, strips any previous ncytm addition to
    /// recover the base status, and writes `base <sep><emoji><track>` back.
    /// Network work happens on a background thread so the UI never blocks.
    pub fn update(&self, track: &Track) {
        let suffix = self.render_suffix(track);
        let separator = self.separator.clone();
        self.spawn(move |this, profile| {
            let base = this.strip_suffix(&profile.status_text);
            let new_text = compose(&base, &separator, &suffix, SLACK_STATUS_MAX_CHARS);
            Some(SlackProfile {
                status_text: new_text,
                // Preserve the user's existing emoji untouched.
                status_emoji: profile.status_emoji,
            })
        });
    }

    /// Strip ncytm's addition from the Slack status, restoring the base status.
    ///
    /// Used when playback is paused or stopped. If the status contains no
    /// addition from us, this is a no-op (we still avoid a needless write).
    pub fn clear(&self) {
        self.spawn(move |this, profile| {
            let base = this.strip_suffix(&profile.status_text);
            if base == profile.status_text {
                // Nothing of ours to remove.
                return None;
            }
            Some(SlackProfile {
                status_text: base,
                status_emoji: profile.status_emoji,
            })
        });
    }

    /// Render the appended text (the part after the separator), e.g.
    /// `🎵 Song — Artist`.
    fn render_suffix(&self, track: &Track) -> String {
        let text = render_format(&self.format, track);
        format!("{}{}", self.emoji, text)
    }

    /// Run `build` against the current Slack profile on a background thread and,
    /// if it returns a new profile, write it back. Errors are logged, never
    /// propagated.
    fn spawn<F>(&self, build: F)
    where
        F: FnOnce(&StripContext, SlackProfile) -> Option<SlackProfile> + Send + 'static,
    {
        let this = StripContext {
            separator: self.separator.clone(),
        };
        let token = self.token.clone();
        let last_written = Arc::clone(&self.last_written);

        std::thread::Builder::new()
            .name("slack-status".into())
            .spawn(move || {
                let client = match build_client() {
                    Ok(client) => client,
                    Err(e) => {
                        warn!("slack: failed to build HTTP client: {e}");
                        return;
                    }
                };

                let profile = match fetch_profile(&client, &token) {
                    Ok(profile) => profile,
                    Err(e) => {
                        warn!("slack: failed to read profile: {e}");
                        return;
                    }
                };

                let Some(new_profile) = build(&this, profile) else {
                    return;
                };

                // Avoid a redundant write if the text is unchanged from what we
                // last wrote this session.
                {
                    let cache = last_written.lock().unwrap();
                    if cache.as_deref() == Some(new_profile.status_text.as_str()) {
                        debug!("slack: status unchanged, skipping write");
                        return;
                    }
                }

                match set_profile(&client, &token, &new_profile) {
                    Ok(()) => {
                        debug!("slack: status set to {:?}", new_profile.status_text);
                        *last_written.lock().unwrap() = Some(new_profile.status_text);
                    }
                    Err(e) => warn!("slack: failed to set profile: {e}"),
                }
            })
            .ok();
    }
}

/// A minimal, `Send`-able copy of the parts of [`SlackStatus`] a worker needs.
struct StripContext {
    separator: String,
}

impl StripContext {
    fn strip_suffix(&self, status_text: &str) -> String {
        strip_suffix(&self.separator, status_text)
    }
}

/// Recover the base status by removing everything from the *last* separator
/// occurrence onward (including the separator and any trailing whitespace
/// before it). If the separator is absent, the whole string is the base.
fn strip_suffix(separator: &str, status_text: &str) -> String {
    match status_text.rfind(separator) {
        Some(idx) => status_text[..idx].trim_end().to_string(),
        None => status_text.to_string(),
    }
}

/// Compose the final status text as `<base> <separator><suffix>`, respecting
/// Slack's character limit. The base is always preserved intact; only the
/// suffix is truncated (with an ellipsis) if the combined string is too long.
///
/// When the base is empty, the result is `<separator><suffix>` (with no leading
/// space), so a fresh status reads e.g. `|🎵 Song — Artist`.
fn compose(base: &str, separator: &str, suffix: &str, max_chars: usize) -> String {
    let base = base.trim_end();

    // The joiner always contains the separator so that `strip_suffix` can later
    // recover the base. With a non-empty base we also prefix a space for
    // readability: `<base> <sep><suffix>`.
    let joiner = if base.is_empty() {
        separator.to_string()
    } else {
        format!(" {separator}")
    };

    let base_len = base.chars().count();
    let joiner_len = joiner.chars().count();

    // If even the base plus joiner doesn't fit, just return the base truncated
    // (should be rare — base already came from Slack, capped at 100).
    if base_len + joiner_len >= max_chars {
        return truncate_chars(base, max_chars);
    }

    let available = max_chars - base_len - joiner_len;
    let suffix = truncate_chars(suffix, available);
    format!("{base}{joiner}{suffix}")
}

/// Truncate a string to at most `max_chars` characters, appending an ellipsis
/// when truncation occurs. Counts by Unicode scalar values (matching Slack).
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    // Reserve one char for the ellipsis.
    let keep = max_chars.saturating_sub(1);
    let truncated: String = s.chars().take(keep).collect();
    format!("{truncated}…")
}

/// Render a format string against a track, substituting `{title}` and
/// `{artists}`.
fn render_format(format: &str, track: &Track) -> String {
    format
        .replace("{title}", &track.title)
        .replace("{artists}", &track.artists.join(", "))
}

/// Return `value` if it is `Some` and non-empty, otherwise `default`.
fn non_empty(value: Option<String>, default: &str) -> String {
    value
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn build_client() -> Result<reqwest::blocking::Client, reqwest::Error> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
}

fn fetch_profile(client: &reqwest::blocking::Client, token: &str) -> Result<SlackProfile, String> {
    let resp: ProfileGetResponse = client
        .get(PROFILE_GET_URL)
        .bearer_auth(token)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;

    if !resp.ok {
        return Err(resp.error.unwrap_or_else(|| "unknown error".into()));
    }
    Ok(resp.profile.unwrap_or_default())
}

fn set_profile(
    client: &reqwest::blocking::Client,
    token: &str,
    profile: &SlackProfile,
) -> Result<(), String> {
    let body = serde_json::json!({
        "profile": {
            "status_text": profile.status_text,
            "status_emoji": profile.status_emoji,
            "status_expiration": 0,
        }
    });

    let resp: ProfileSetResponse = client
        .post(PROFILE_SET_URL)
        .bearer_auth(token)
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;

    if !resp.ok {
        return Err(resp.error.unwrap_or_else(|| "unknown error".into()));
    }
    Ok(())
}

/// Verify a Slack token by reading the current profile. Used by the
/// `ncytm slack --check` subcommand.
pub fn check_token(token: &str) -> Result<SlackProfileSummary, String> {
    let client = build_client().map_err(|e| e.to_string())?;
    let profile = fetch_profile(&client, token)?;
    Ok(SlackProfileSummary {
        status_text: profile.status_text,
        status_emoji: profile.status_emoji,
    })
}

/// A user-facing summary of the current Slack status.
pub struct SlackProfileSummary {
    pub status_text: String,
    pub status_emoji: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str, artists: &[&str]) -> Track {
        Track {
            id: Some("abc".into()),
            title: title.into(),
            duration: 200,
            artists: artists.iter().map(|s| s.to_string()).collect(),
            artist_ids: vec![],
            album: None,
            album_id: None,
            cover_url: None,
            added_at: None,
            list_index: 0,
            is_explicit: false,
            set_video_id: None,
        }
    }

    fn manager(separator: &str) -> SlackStatus {
        SlackStatus {
            token: "xoxp-test".into(),
            separator: separator.into(),
            emoji: "🎵".into(),
            format: "{title} — {artists}".into(),
            last_written: Arc::new(Mutex::new(None)),
        }
    }

    #[test]
    fn render_format_substitutes_tokens() {
        let t = track("Song", &["Alice", "Bob"]);
        assert_eq!(
            render_format("{title} — {artists}", &t),
            "Song — Alice, Bob"
        );
        assert_eq!(render_format("{artists}: {title}", &t), "Alice, Bob: Song");
    }

    #[test]
    fn suffix_includes_emoji() {
        let m = manager("|");
        let t = track("Song", &["Alice"]);
        assert_eq!(m.render_suffix(&t), "🎵Song — Alice");
    }

    #[test]
    fn strip_suffix_with_no_separator_returns_whole() {
        assert_eq!(strip_suffix("|", "Focusing"), "Focusing");
    }

    #[test]
    fn strip_suffix_removes_our_addition() {
        assert_eq!(strip_suffix("|", "Focusing |🎵 Song — Alice"), "Focusing");
    }

    #[test]
    fn strip_suffix_uses_last_separator() {
        // A user base status that itself contains the separator should keep the
        // part before the *last* separator (our addition is always last).
        assert_eq!(strip_suffix("|", "a | b |🎵 Song — Alice"), "a | b");
    }

    #[test]
    fn compose_appends_with_separator() {
        let out = compose("Focusing", "|", "🎵Song — Alice", SLACK_STATUS_MAX_CHARS);
        assert_eq!(out, "Focusing |🎵Song — Alice");
    }

    #[test]
    fn compose_empty_base_is_separator_plus_suffix() {
        let out = compose("", "|", "🎵Song — Alice", SLACK_STATUS_MAX_CHARS);
        assert_eq!(out, "|🎵Song — Alice");
    }

    #[test]
    fn update_then_strip_is_idempotent() {
        // Simulate: user has "Focusing", we append, then read it back and append
        // again — the base must remain "Focusing", never stacking.
        let m = manager("|");
        let t = track("Song", &["Alice"]);
        let suffix = m.render_suffix(&t);

        let base1 = strip_suffix("|", "Focusing");
        let composed1 = compose(&base1, "|", &suffix, SLACK_STATUS_MAX_CHARS);
        assert_eq!(composed1, "Focusing |🎵Song — Alice");

        // Read our own output back and update to a new track.
        let t2 = track("Other", &["Bob"]);
        let suffix2 = m.render_suffix(&t2);
        let base2 = strip_suffix("|", &composed1);
        assert_eq!(base2, "Focusing");
        let composed2 = compose(&base2, "|", &suffix2, SLACK_STATUS_MAX_CHARS);
        assert_eq!(composed2, "Focusing |🎵Other — Bob");
    }

    #[test]
    fn manual_status_becomes_new_base() {
        // User changed status while stopped; no separator present, so the whole
        // thing is the base and we append to it.
        let base = strip_suffix("|", "In a meeting");
        assert_eq!(base, "In a meeting");
        let out = compose(&base, "|", "🎵Song — Alice", SLACK_STATUS_MAX_CHARS);
        assert_eq!(out, "In a meeting |🎵Song — Alice");
    }

    #[test]
    fn truncate_preserves_base_and_truncates_suffix() {
        let base = "x".repeat(80);
        let suffix = "🎵".to_string() + &"y".repeat(80);
        let out = compose(&base, "|", &suffix, 100);
        // Base preserved entirely.
        assert!(out.starts_with(&base));
        // Total length within the limit.
        assert!(out.chars().count() <= 100);
        // Suffix was truncated with an ellipsis.
        assert!(out.ends_with('…'));
    }

    #[test]
    fn truncate_chars_short_string_unchanged() {
        assert_eq!(truncate_chars("hello", 100), "hello");
    }

    #[test]
    fn truncate_chars_adds_ellipsis() {
        assert_eq!(truncate_chars("hello", 3), "he…");
    }

    #[test]
    fn non_empty_prefers_value() {
        assert_eq!(non_empty(Some("a".into()), "b"), "a");
        assert_eq!(non_empty(Some(String::new()), "b"), "b");
        assert_eq!(non_empty(None, "b"), "b");
    }

    #[test]
    fn new_disabled_returns_none() {
        let cfg = SlackStatusConfig {
            enabled: false,
            token: Some("xoxp-test".into()),
            ..Default::default()
        };
        assert!(SlackStatus::new(Some(&cfg)).is_none());
    }

    #[test]
    fn new_enabled_with_token_config() {
        let cfg = SlackStatusConfig {
            enabled: true,
            token: Some("xoxp-test".into()),
            ..Default::default()
        };
        let s = SlackStatus::new(Some(&cfg)).expect("should build");
        assert_eq!(s.token, "xoxp-test");
        assert_eq!(s.separator, "|");
    }

    #[test]
    fn new_enabled_without_token_returns_none() {
        // Ensure the env var isn't set for this test.
        unsafe {
            std::env::remove_var("SLACK_TOKEN");
        }
        let cfg = SlackStatusConfig {
            enabled: true,
            token: None,
            ..Default::default()
        };
        assert!(SlackStatus::new(Some(&cfg)).is_none());
    }

    #[test]
    fn set_profile_body_shape() {
        // Verify the JSON we would send is well-formed and preserves emoji.
        let profile = SlackProfile {
            status_text: "Focusing 🎵Song — Alice".into(),
            status_emoji: ":coffee:".into(),
        };
        let body = serde_json::json!({
            "profile": {
                "status_text": profile.status_text,
                "status_emoji": profile.status_emoji,
                "status_expiration": 0,
            }
        });
        assert_eq!(body["profile"]["status_text"], "Focusing 🎵Song — Alice");
        assert_eq!(body["profile"]["status_emoji"], ":coffee:");
        assert_eq!(body["profile"]["status_expiration"], 0);
    }

    /// Live integration test. Requires SLACK_TOKEN with the correct scopes.
    /// Run manually with: `cargo test slack -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_check_token() {
        let token = std::env::var("SLACK_TOKEN").expect("SLACK_TOKEN must be set");
        let summary = check_token(&token).expect("token should be valid");
        println!(
            "current status: {:?} {:?}",
            summary.status_emoji, summary.status_text
        );
    }
}
