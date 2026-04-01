/// UTF-8 safe input buffer with cursor management and history
pub struct InputState {
    pub buffer: String,
    pub cursor: usize, // Byte position (always at char boundary)
    pub history: Vec<String>,
    pub history_index: Option<usize>,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
        }
    }

    /// Insert a char at the current cursor position (UTF-8 safe)
    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Delete the char before the cursor (backspace, UTF-8 safe)
    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        // Find the previous char boundary
        let prev = self.buffer[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.buffer.remove(prev);
        self.cursor = prev;
        true
    }

    /// Delete the char at the cursor position (delete key, UTF-8 safe)
    pub fn delete(&mut self) -> bool {
        if self.cursor >= self.buffer.len() {
            return false;
        }
        self.buffer.remove(self.cursor);
        true
    }

    /// Move cursor one char left (UTF-8 safe)
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.buffer[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move cursor one char right (UTF-8 safe)
    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.len() {
            self.cursor = self.buffer[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.buffer.len());
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.buffer.len();
    }

    /// Display width of text before cursor (for rendering)
    pub fn cursor_display_width(&self) -> usize {
        use unicode_width::UnicodeWidthStr;
        UnicodeWidthStr::width(&self.buffer[..self.cursor])
    }

    pub fn submit(&mut self) -> String {
        let text = self.buffer.trim().to_string();
        if !text.is_empty() {
            self.history.push(text.clone());
        }
        self.buffer.clear();
        self.cursor = 0;
        self.history_index = None;
        text
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
        self.history_index = None;
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.history_index {
            Some(i) if i > 0 => i - 1,
            None if !self.history.is_empty() => self.history.len() - 1,
            _ => return,
        };
        self.history_index = Some(idx);
        self.buffer = self.history[idx].clone();
        self.cursor = self.buffer.len();
    }

    pub fn history_next(&mut self) {
        let idx = match self.history_index {
            Some(i) => i + 1,
            None => return,
        };
        if idx < self.history.len() {
            self.history_index = Some(idx);
            self.buffer = self.history[idx].clone();
        } else {
            self.history_index = None;
            self.buffer.clear();
        }
        self.cursor = self.buffer.len();
    }
}
