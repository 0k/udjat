//! Harness kinds and their recognition from a session id.

use anyhow::{Result, bail};
use serde::Serialize;

/// The harness program a session belongs to.
///
/// The serde form (`claude_code`, `open_code`, `pi`) and [`Self::as_str`]
/// (`claudecode`, `opencode`, `pi`) are both part of consumers' output
/// formats and must not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessKind {
    ClaudeCode,
    OpenCode,
    Pi,
}

impl HarnessKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            HarnessKind::ClaudeCode => "claudecode",
            HarnessKind::OpenCode => "opencode",
            HarnessKind::Pi => "pi",
        }
    }
}

impl std::fmt::Display for HarnessKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Detect the harness kind of a session id from its format.
///
/// Recognised formats:
/// - `ses_*` prefix → OpenCode
/// - UUIDv4 (version digit `4`) → Claude Code
/// - UUIDv7 (version digit `7`) → Pi
///
/// Anything else (malformed strings, unknown UUID versions) returns an
/// error.  Loud failure here is intentional: unrecognised IDs are almost
/// always user error or a sign that a new harness exists which the
/// codebase has not yet been taught about.
pub fn detect_kind(session_id: &str) -> Result<HarnessKind> {
    if session_id.starts_with("ses_") {
        return Ok(HarnessKind::OpenCode);
    }
    if !looks_like_hyphenated_uuid(session_id) {
        bail!(
            "Unrecognised session ID format: {:?} (expected ses_* for OpenCode, \
             UUIDv4 for Claude Code, or UUIDv7 for pi)",
            session_id
        );
    }
    // Version nibble is the 15th character (index 14) of a hyphenated UUID.
    match session_id.as_bytes()[14] {
        b'4' => Ok(HarnessKind::ClaudeCode),
        b'7' => Ok(HarnessKind::Pi),
        other => bail!(
            "Unsupported UUID version {} in session ID {:?} (only v4=Claude Code, \
             v7=pi are recognised)",
            other as char,
            session_id
        ),
    }
}

/// Cheap structural check for a hyphenated 36-char UUID.
fn looks_like_hyphenated_uuid(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    let bytes = s.as_bytes();
    bytes[8] == b'-' && bytes[13] == b'-' && bytes[18] == b'-' && bytes[23] == b'-'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_opencode() {
        assert_eq!(detect_kind("ses_abc123").unwrap(), HarnessKind::OpenCode);
        assert_eq!(detect_kind("ses_").unwrap(), HarnessKind::OpenCode);
    }

    #[test]
    fn detects_claudecode_uuidv4() {
        assert_eq!(
            detect_kind("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            HarnessKind::ClaudeCode
        );
    }

    #[test]
    fn detects_pi_uuidv7() {
        assert_eq!(
            detect_kind("019dddbf-6e66-7709-9a3b-b5a18736f890").unwrap(),
            HarnessKind::Pi
        );
    }

    #[test]
    fn rejects_malformed() {
        assert!(detect_kind("some-uuid").is_err());
        assert!(detect_kind("").is_err());
        assert!(detect_kind("not-a-uuid-at-all-really-not-one-here").is_err());
    }

    #[test]
    fn rejects_unknown_uuid_version() {
        assert!(detect_kind("550e8400-e29b-11d4-a716-446655440000").is_err());
    }

    #[test]
    fn string_forms_are_stable() {
        let all = [
            HarnessKind::ClaudeCode,
            HarnessKind::OpenCode,
            HarnessKind::Pi,
        ];
        let as_str: Vec<_> = all.iter().map(HarnessKind::as_str).collect();
        let display: Vec<_> = all.iter().map(ToString::to_string).collect();
        let serde: Vec<_> = all
            .iter()
            .map(|k| serde_json::to_value(k).unwrap())
            .collect();
        assert_eq!(as_str, ["claudecode", "opencode", "pi"]);
        assert_eq!(display, ["claudecode", "opencode", "pi"]);
        assert_eq!(serde, ["claude_code", "open_code", "pi"]);
    }
}
