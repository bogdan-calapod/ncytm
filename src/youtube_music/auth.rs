//! Authentication helpers for YouTube Music API.
//!
//! Provides SAPISID hash generation required for authenticated API requests.

use sha1::{Digest, Sha1};
use std::time::{SystemTime, UNIX_EPOCH};

/// The origin URL for YouTube Music, used in SAPISID hash generation.
pub const YOUTUBE_MUSIC_ORIGIN: &str = "https://music.youtube.com";

/// Generate a SAPISID hash for YouTube API authentication.
///
/// The SAPISID hash is used in the `Authorization` header for authenticated requests.
/// It has the format: `SAPISIDHASH <timestamp>_<sha1_hash>`
///
/// The SHA1 hash is computed from: `<timestamp> <SAPISID> <origin>`
///
/// # Arguments
///
/// * `sapisid` - The SAPISID cookie value
/// * `origin` - The origin URL (typically `https://music.youtube.com`)
///
/// # Returns
///
/// A string in the format `SAPISIDHASH <timestamp>_<hash>`
pub fn generate_sapisid_hash(sapisid: &str, origin: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before Unix epoch")
        .as_secs();

    generate_sapisid_hash_with_timestamp(sapisid, origin, timestamp)
}

/// Generate a SAPISID hash with a specific timestamp (for testing).
///
/// This is the same as `generate_sapisid_hash` but allows specifying the timestamp
/// for deterministic testing.
pub fn generate_sapisid_hash_with_timestamp(sapisid: &str, origin: &str, timestamp: u64) -> String {
    // Create the string to hash: "<timestamp> <SAPISID> <origin>"
    let to_hash = format!("{} {} {}", timestamp, sapisid, origin);

    // Compute SHA1 hash
    let mut hasher = Sha1::new();
    hasher.update(to_hash.as_bytes());
    let hash_result = hasher.finalize();
    let hash_hex = hex::encode(hash_result);

    // Format as "SAPISIDHASH <timestamp>_<hash>"
    format!("SAPISIDHASH {}_{}", timestamp, hash_hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Validate that a SAPISID hash has the correct format.
    /// Used for testing hash generation.
    fn is_valid_sapisid_hash_format(hash: &str) -> bool {
        // Format should be: SAPISIDHASH <timestamp>_<40-char-hex>
        let parts: Vec<&str> = hash.splitn(2, ' ').collect();
        if parts.len() != 2 || parts[0] != "SAPISIDHASH" {
            return false;
        }

        let remainder = parts[1];
        let hash_parts: Vec<&str> = remainder.splitn(2, '_').collect();
        if hash_parts.len() != 2 {
            return false;
        }

        // Timestamp should be numeric
        if hash_parts[0].parse::<u64>().is_err() {
            return false;
        }

        // Hash should be 40 hex characters (SHA1)
        let hex_hash = hash_parts[1];
        hex_hash.len() == 40 && hex_hash.chars().all(|c| c.is_ascii_hexdigit())
    }

    #[test]
    fn test_generate_sapisid_hash_known_values() {
        // Test with known values to ensure deterministic output
        let sapisid = "abc123xyz/ABCDEFGHIJ";
        let origin = "https://music.youtube.com";
        let timestamp = 1700000000u64;

        let hash = generate_sapisid_hash_with_timestamp(sapisid, origin, timestamp);

        // Verify format
        assert!(hash.starts_with("SAPISIDHASH 1700000000_"));

        // The hash should be consistent
        let hash2 = generate_sapisid_hash_with_timestamp(sapisid, origin, timestamp);
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_sapisid_hash_format() {
        let sapisid = "test_sapisid_value";
        let origin = "https://music.youtube.com";
        let timestamp = 1234567890u64;

        let hash = generate_sapisid_hash_with_timestamp(sapisid, origin, timestamp);

        // Check format: "SAPISIDHASH <timestamp>_<40-char-hex>"
        assert!(hash.starts_with("SAPISIDHASH "));

        let remainder = &hash["SAPISIDHASH ".len()..];
        let parts: Vec<&str> = remainder.split('_').collect();

        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "1234567890");
        assert_eq!(parts[1].len(), 40); // SHA1 produces 40 hex chars
        assert!(parts[1].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_different_inputs_produce_different_hashes() {
        let origin = "https://music.youtube.com";
        let timestamp = 1700000000u64;

        let hash1 = generate_sapisid_hash_with_timestamp("sapisid1", origin, timestamp);
        let hash2 = generate_sapisid_hash_with_timestamp("sapisid2", origin, timestamp);

        // Different SAPISID should produce different hashes
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_different_timestamps_produce_different_hashes() {
        let sapisid = "same_sapisid";
        let origin = "https://music.youtube.com";

        let hash1 = generate_sapisid_hash_with_timestamp(sapisid, origin, 1700000000);
        let hash2 = generate_sapisid_hash_with_timestamp(sapisid, origin, 1700000001);

        // Different timestamps should produce different hashes
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_is_valid_sapisid_hash_format_valid() {
        let hash = "SAPISIDHASH 1700000000_a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        assert!(is_valid_sapisid_hash_format(hash));
    }

    #[test]
    fn test_is_valid_sapisid_hash_format_invalid_prefix() {
        let hash = "INVALID 1700000000_a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        assert!(!is_valid_sapisid_hash_format(hash));
    }

    #[test]
    fn test_is_valid_sapisid_hash_format_invalid_timestamp() {
        let hash = "SAPISIDHASH notanumber_a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        assert!(!is_valid_sapisid_hash_format(hash));
    }

    #[test]
    fn test_is_valid_sapisid_hash_format_invalid_hash_length() {
        let hash = "SAPISIDHASH 1700000000_tooshort";
        assert!(!is_valid_sapisid_hash_format(hash));
    }

    #[test]
    fn test_is_valid_sapisid_hash_format_invalid_hash_chars() {
        let hash = "SAPISIDHASH 1700000000_g1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"; // 'g' is not hex
        assert!(!is_valid_sapisid_hash_format(hash));
    }

    #[test]
    fn test_generate_sapisid_hash_uses_current_time() {
        // This test verifies that generate_sapisid_hash uses current time
        let sapisid = "test_sapisid";
        let origin = "https://music.youtube.com";

        let hash = generate_sapisid_hash(sapisid, origin);

        // Extract timestamp from hash
        let parts: Vec<&str> = hash["SAPISIDHASH ".len()..].split('_').collect();
        let hash_timestamp: u64 = parts[0].parse().unwrap();

        // Should be within reasonable range of current time (within 5 seconds)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        assert!(hash_timestamp <= now);
        assert!(hash_timestamp >= now - 5);
    }
}
