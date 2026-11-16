//! UTF-8 fast paths over input text.

/// Returns `true` for bytes allowed in a bare TOML key: `A-Za-z0-9_-`.
/// All of them are ASCII.
#[inline]
pub(crate) fn is_keylike_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// Returns the run of bare-key bytes starting at `start` as a `&str`,
/// together with the end offset of the run. If `start` is out of range the
/// run is empty.
#[inline]
pub(crate) fn keylike_run(text: &str, start: usize) -> (&str, usize) {
    let bytes = text.as_bytes();
    let mut end = start;
    while let Some(&b) = bytes.get(end) {
        if !is_keylike_byte(b) {
            break;
        }
        end += 1;
    }
    let run = bytes.get(start..end).unwrap_or_default();
    // SAFETY: every byte of `run` satisfied `is_keylike_byte`, which only
    // accepts ASCII, and a sequence of ASCII bytes is valid UTF-8.
    (unsafe { std::str::from_utf8_unchecked(run) }, end)
}

#[cfg(test)]
mod tests {
    use super::keylike_run;

    #[test]
    fn stops_at_non_key_bytes() {
        assert_eq!(keylike_run("abc-d_1=2", 0), ("abc-d_1", 7));
        assert_eq!(keylike_run("aé", 0), ("a", 1));
        // Starting inside a multi-byte character yields an empty run.
        assert_eq!(keylike_run("é", 1), ("", 1));
        assert_eq!(keylike_run("ab", 9), ("", 9));
    }
}
