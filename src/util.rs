/// Parse an ASCII-decimal byte slice into a numeric type.
pub(crate) fn parse_bytes<T: std::str::FromStr>(b: &[u8]) -> Option<T> {
    std::str::from_utf8(b).ok()?.parse().ok()
}

/// Remove trailing ASCII spaces from a byte vec in-place.
pub(crate) fn trim_spaces_end(v: &mut Vec<u8>) {
    while v.last() == Some(&b' ') {
        v.pop();
    }
}

/// Extract a frequency in MHz from the start of a comment field.
///
/// Looks for a decimal number immediately followed by `MHz` (case-sensitive)
/// at the beginning of the slice. Returns `None` if not found.
/// The comment field is not modified — it is preserved verbatim for round-trip fidelity.
///
/// Example: `b"146.520MHz T100 comment"` → `Some(146.52)`
pub(crate) fn extract_frequency_mhz(comment: &[u8]) -> Option<f32> {
    // Find "MHz"
    let mhz_pos = comment.windows(3).position(|w| w == b"MHz")?;
    if mhz_pos == 0 {
        return None; // No digits before MHz
    }
    // Walk back to the start of the number (digits and `.`)
    let num_bytes = &comment[..mhz_pos];
    // Ensure the number is at the very start (optionally preceded by whitespace)
    let trimmed = num_bytes
        .iter()
        .position(|&b| b != b' ')
        .map(|i| &num_bytes[i..])
        .unwrap_or(num_bytes);
    if trimmed.is_empty() {
        return None;
    }
    // The number must consist only of digits and at most one `.`
    if !trimmed.iter().all(|&b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    std::str::from_utf8(trimmed).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_basic() {
        assert_eq!(
            extract_frequency_mhz(b"146.520MHz T100 comment"),
            Some(146.52)
        );
    }

    #[test]
    fn frequency_no_decimal() {
        assert_eq!(extract_frequency_mhz(b"146MHz comment"), Some(146.0));
    }

    #[test]
    fn frequency_not_at_start() {
        assert_eq!(extract_frequency_mhz(b"comment 146.520MHz"), None);
    }

    #[test]
    fn frequency_absent() {
        assert_eq!(extract_frequency_mhz(b"no frequency here"), None);
    }
}
