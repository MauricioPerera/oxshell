/// Move cursor forward by `count` words. Returns new byte position.
pub fn word_forward(buffer: &str, cursor: usize, count: usize) -> usize {
    let mut pos = cursor;
    for _ in 0..count {
        // Skip current word chars
        while pos < buffer.len() && !is_word_boundary(buffer, pos) {
            pos = next_char_boundary(buffer, pos);
        }
        // Skip whitespace
        while pos < buffer.len() && buffer[pos..].starts_with(char::is_whitespace) {
            pos = next_char_boundary(buffer, pos);
        }
    }
    pos.min(buffer.len())
}

/// Move cursor backward by `count` words. Returns new byte position.
pub fn word_back(buffer: &str, cursor: usize, count: usize) -> usize {
    let mut pos = cursor;
    for _ in 0..count {
        // Skip whitespace backward
        while pos > 0 && buffer[..pos].ends_with(char::is_whitespace) {
            pos = prev_char_boundary(buffer, pos);
        }
        // Skip word chars backward
        while pos > 0 && !is_word_boundary_before(buffer, pos) {
            pos = prev_char_boundary(buffer, pos);
        }
    }
    pos
}

/// Move cursor to end of current/next word. Returns new byte position.
pub fn word_end(buffer: &str, cursor: usize, count: usize) -> usize {
    let mut pos = if cursor < buffer.len() {
        next_char_boundary(buffer, cursor)
    } else {
        cursor
    };

    for _ in 0..count {
        // Skip whitespace
        while pos < buffer.len() && buffer[pos..].starts_with(char::is_whitespace) {
            pos = next_char_boundary(buffer, pos);
        }
        // Move to end of word
        while pos < buffer.len() && !is_word_boundary(buffer, pos) {
            let next = next_char_boundary(buffer, pos);
            if next >= buffer.len() || is_word_boundary(buffer, next) {
                break;
            }
            pos = next;
        }
    }
    pos.min(buffer.len().saturating_sub(1))
}

/// Find the first non-blank character position
pub fn first_non_blank(buffer: &str) -> usize {
    buffer
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(i, _)| i)
        .unwrap_or(0)
}

// ─── Helpers ────────────────────────────────────────────

fn is_word_boundary(buffer: &str, pos: usize) -> bool {
    if pos >= buffer.len() {
        return true;
    }
    let c = buffer[pos..].chars().next().unwrap_or(' ');
    c.is_whitespace() || is_punctuation(c)
}

fn is_word_boundary_before(buffer: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let c = buffer[..pos].chars().next_back().unwrap_or(' ');
    c.is_whitespace() || is_punctuation(c)
}

fn is_punctuation(c: char) -> bool {
    matches!(
        c,
        '.' | ',' | ';' | ':' | '!' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\''
            | '/' | '\\' | '|' | '<' | '>' | '+' | '-' | '=' | '*' | '&' | '%' | '#' | '@'
    )
}

fn next_char_boundary(buffer: &str, pos: usize) -> usize {
    let mut p = pos + 1;
    while p < buffer.len() && !buffer.is_char_boundary(p) {
        p += 1;
    }
    p
}

fn prev_char_boundary(buffer: &str, pos: usize) -> usize {
    let mut p = pos.saturating_sub(1);
    while p > 0 && !buffer.is_char_boundary(p) {
        p -= 1;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_forward() {
        assert_eq!(word_forward("hello world", 0, 1), 6);
        assert_eq!(word_forward("hello world", 0, 2), 11);
        assert_eq!(word_forward("abc def ghi", 0, 1), 4);
    }

    #[test]
    fn test_word_back() {
        assert_eq!(word_back("hello world", 11, 1), 6);
        assert_eq!(word_back("hello world", 6, 1), 0);
    }

    #[test]
    fn test_first_non_blank() {
        assert_eq!(first_non_blank("  hello"), 2);
        assert_eq!(first_non_blank("hello"), 0);
        assert_eq!(first_non_blank("   "), 0);
    }

    #[test]
    fn test_word_end() {
        let pos = word_end("hello world", 0, 1);
        assert!(pos >= 4 && pos <= 5); // End of "hello"
    }

    #[test]
    fn test_empty_buffer() {
        assert_eq!(word_forward("", 0, 1), 0);
        assert_eq!(word_back("", 0, 1), 0);
        assert_eq!(first_non_blank(""), 0);
    }
}
