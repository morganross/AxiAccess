//! Captured immutable UTF-8 pages. A cursor cannot join text from two revisions.
use crate::model::{AccessError, MAX_TEXT_PAGE};
pub fn page(
    text: &str,
    offset: usize,
    max_bytes: usize,
) -> Result<(&str, Option<usize>), AccessError> {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return Err(AccessError::new(
            "invalid_cursor",
            "Text offset is not a valid UTF-8 boundary",
        ));
    }
    let mut end = offset
        .saturating_add(max_bytes.clamp(4, MAX_TEXT_PAGE))
        .min(text.len());
    while end > offset && !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok((&text[offset..end], (end < text.len()).then_some(end)))
}
pub fn truncate_utf8(text: &mut String, max: usize) -> bool {
    if text.len() <= max {
        return false;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_pages_round_trip() {
        let s = "a🦉漢é".repeat(100);
        let mut offset = 0;
        let mut output = String::new();
        loop {
            let (p, n) = page(&s, offset, 7).unwrap();
            output.push_str(p);
            match n {
                Some(v) => offset = v,
                None => break,
            }
        }
        assert_eq!(output, s);
    }
    #[test]
    fn rejects_mid_character() {
        assert!(page("🦉", 1, 4).is_err());
    }
}
