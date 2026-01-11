use std::collections::HashSet;

use data_encoding::BASE32_NOPAD;
use once_cell::sync::Lazy;
use rand::{thread_rng, RngCore};
use regex::Regex;

const MIN_ID_LEN: usize = 3;
const MAX_ID_LEN: usize = 64;

#[derive(Debug, PartialEq, Eq)]
pub enum IdError {
    Empty,
    TooShort,
    TooLong,
    InvalidFormat,
    Reserved,
}

static ID_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z0-9][a-z0-9_-]{2,63}$").expect("regex must compile"));

static RESERVED: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "list", "add", "rm", "connect", "exec", "run", "doctor", "secret", "config", "push",
        "pull", "xfer", "test", "ui", "agent",
    ]
    .into_iter()
    .collect()
});

/// Normalize an identifier by lowercasing it.
pub fn normalize_id(input: &str) -> String {
    input.to_lowercase()
}

/// Validate an identifier against repository rules.
pub fn validate_id(candidate: &str) -> Result<(), IdError> {
    if candidate.is_empty() {
        return Err(IdError::Empty);
    }
    if candidate.len() < MIN_ID_LEN {
        return Err(IdError::TooShort);
    }
    if candidate.len() > MAX_ID_LEN {
        return Err(IdError::TooLong);
    }
    if RESERVED.contains(candidate) {
        return Err(IdError::Reserved);
    }
    if !ID_REGEX.is_match(candidate) {
        return Err(IdError::InvalidFormat);
    }
    Ok(())
}

/// Generate a random ID with the given prefix using lowercase base32 characters.
pub fn generate_id(prefix: &str) -> String {
    let mut rng = thread_rng();
    let mut bytes = [0u8; 5];
    loop {
        rng.fill_bytes(&mut bytes);
        let encoded = BASE32_NOPAD.encode(&bytes).to_lowercase();
        let candidate = format!("{prefix}{encoded}");
        if validate_id(&candidate).is_ok() {
            return candidate;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lowercases() {
        assert_eq!(normalize_id("AbC_123"), "abc_123");
    }

    #[test]
    fn rejects_empty_and_length() {
        assert_eq!(validate_id(""), Err(IdError::Empty));
        assert_eq!(validate_id("a"), Err(IdError::TooShort));
        let long = "a".repeat(65);
        assert_eq!(validate_id(&long), Err(IdError::TooLong));
    }

    #[test]
    fn rejects_reserved_and_format() {
        assert_eq!(validate_id("list"), Err(IdError::Reserved));
        assert_eq!(validate_id("Bad*"), Err(IdError::InvalidFormat));
    }

    #[test]
    fn accepts_valid_ids() {
        assert!(validate_id("p_valid-id_123").is_ok());
    }

    #[test]
    fn generates_prefixed_ids() {
        let id = generate_id("p_");
        assert!(id.starts_with("p_"));
        assert!(id.len() >= 3);
        assert!(id.len() <= 64);
        assert!(validate_id(&id).is_ok());
    }
}
