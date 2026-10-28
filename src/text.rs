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

/// Return the last `max_bytes` UTF-8 slice of `text` (for `tail: true` requests),
/// snapping cleanly to a UTF-8 char boundary and preferring a line boundary when available.
pub fn tail_page(text: &str, max_bytes: usize) -> (&str, usize) {
    let cap = max_bytes.clamp(4, MAX_TEXT_PAGE);
    if text.len() <= cap {
        return (text, 0);
    }
    let mut start = text.len() - cap;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    // If there is a newline within the first 128 bytes of the slice, snap to the line start
    if let Some(rel_nl) = text[start..].find('\n') {
        if rel_nl < 128 && start + rel_nl + 1 < text.len() {
            start += rel_nl + 1;
        }
    }
    (&text[start..], start)
}

/// Search `text` case-insensitively for `query`, returning matching lines with ±2 context lines
/// and byte offsets, bounded to `max_bytes`.
pub fn search_lines(text: &str, query: &str, max_bytes: usize) -> (String, Vec<serde_json::Value>) {
    let cap = max_bytes.clamp(64, MAX_TEXT_PAGE);
    let needle = query.to_lowercase();
    let lines: Vec<&str> = text.lines().collect();
    let mut matches = Vec::new();
    let mut byte_offsets = Vec::with_capacity(lines.len());
    let mut cur_offset = 0usize;
    for line in &lines {
        byte_offsets.push(cur_offset);
        cur_offset += line.len() + 1;
    }
    let mut excerpt = String::new();
    let mut last_included: Option<usize> = None;
    for (idx, line) in lines.iter().enumerate() {
        if line.to_lowercase().contains(&needle) {
            matches.push(serde_json::json!({
                "lineNumber": idx + 1,
                "byteOffset": byte_offsets[idx],
                "line": line,
            }));
            let start_ctx = idx.saturating_sub(2);
            let end_ctx = (idx + 2).min(lines.len().saturating_sub(1));
            if let Some(prev) = last_included {
                if start_ctx > prev + 1 {
                    let sep = "---\n";
                    if excerpt.len() + sep.len() <= cap {
                        excerpt.push_str(sep);
                    }
                }
            }
            for ctx_i in start_ctx..=end_ctx {
                if last_included.is_some_and(|prev| ctx_i <= prev) {
                    continue;
                }
                let formatted = format!("L{}: {}\n", ctx_i + 1, lines[ctx_i]);
                if excerpt.len() + formatted.len() > cap {
                    break;
                }
                excerpt.push_str(&formatted);
                last_included = Some(ctx_i);
            }
        }
        if matches.len() >= 100 || excerpt.len() >= cap {
            break;
        }
    }
    (excerpt, matches)
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
    #[test]
    fn tail_and_search_respect_utf8_and_context() {
        let doc = "line1: start\nline2: middle\nline3: ERROR owl 🦉 failed\nline4: end\n";
        let (tail, start) = tail_page(doc, 36);
        assert!(tail.ends_with("line4: end\n"));
        assert!(doc.is_char_boundary(start));
        let (excerpt, matches) = search_lines(doc, "error owl", 1024);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["lineNumber"], 3);
        assert!(excerpt.contains("L3: line3: ERROR owl 🦉 failed"));
    }
}
