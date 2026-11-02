//! What the outbound JSON feeds share: a capped read of an answer body, a text field, and
//! the ranges a sportsbook's numbers must lie in to be kept.

use serde_json::Value;

use crate::application::ports::SourceError;

/// Provider error messages are cut to this length before they reach the log.
pub const MAX_ERROR_CHARS: usize = 300;

/// The answer's status and body, read in chunks and refused past `max_bytes`. A transport
/// error loses its URL, which may carry a key.
pub async fn read_body(
    resp: reqwest::Response,
    max_bytes: usize,
) -> Result<(reqwest::StatusCode, Vec<u8>), SourceError> {
    let status = resp.status();
    let mut resp = resp;
    let mut bytes: Vec<u8> = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| SourceError::Transport(e.without_url().to_string()))?
    {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > max_bytes {
            return Err(SourceError::InvalidPayload(format!(
                "answer longer than {max_bytes} bytes"
            )));
        }
    }
    Ok((status, bytes))
}

/// The error a failed answer becomes: its status and the start of its body.
pub fn rejected(status: reqwest::StatusCode, body: &[u8]) -> SourceError {
    SourceError::Rejected {
        status: status.as_u16(),
        message: String::from_utf8_lossy(body)
            .chars()
            .take(MAX_ERROR_CHARS)
            .collect(),
    }
}

/// A non-empty text field, trimmed.
pub fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// A price in American odds: at least 100 either way (EVEN is +100), at most 100,000.
pub fn usable_odds(o: f64) -> bool {
    o.is_finite() && (100.0..=100_000.0).contains(&o.abs())
}

/// A spread: no wider than any game's, 60 points either way.
pub fn usable_spread(s: f64) -> bool {
    s.is_finite() && s.abs() <= 60.0
}

/// A total: an NBA total lies between 100 and 400 points.
pub fn usable_total(t: f64) -> bool {
    t.is_finite() && (100.0..=400.0).contains(&t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ranges_keep_a_sportsbook_number_and_drop_the_rest() {
        assert!(usable_odds(-110.0) && usable_odds(100.0) && usable_odds(100_000.0));
        assert!(!usable_odds(-50.0) && !usable_odds(1e9) && !usable_odds(f64::NAN));
        assert!(usable_spread(-2.5) && usable_spread(0.0) && !usable_spread(70.0));
        assert!(usable_total(219.5) && !usable_total(50.0) && !usable_total(f64::INFINITY));
        let v: Value = serde_json::json!({"a": " x ", "b": "  ", "c": 1});
        assert_eq!(text(&v, "a").as_deref(), Some("x"));
        assert!(text(&v, "b").is_none() && text(&v, "c").is_none());
        let e = rejected(
            reqwest::StatusCode::UNAUTHORIZED,
            "x".repeat(400).as_bytes(),
        );
        assert!(
            matches!(e, SourceError::Rejected { status: 401, ref message } if message.len() == MAX_ERROR_CHARS)
        );
    }
}
