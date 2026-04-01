mod input_state {
    #[test]
    fn test_insert_ascii() {
        let mut input = InputState::new();
        input.insert_char('h');
        input.insert_char('i');
        assert_eq!(input.buffer, "hi");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_insert_emoji() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.insert_char('🦀'); // 4-byte char
        input.insert_char('b');
        assert_eq!(input.buffer, "a🦀b");
        assert_eq!(input.cursor, 6); // 1 + 4 + 1
    }

    #[test]
    fn test_backspace_ascii() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.insert_char('b');
        input.insert_char('c');
        assert!(input.backspace());
        assert_eq!(input.buffer, "ab");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_backspace_emoji() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.insert_char('🦀');
        assert!(input.backspace());
        assert_eq!(input.buffer, "a");
        assert_eq!(input.cursor, 1);
    }

    #[test]
    fn test_backspace_empty() {
        let mut input = InputState::new();
        assert!(!input.backspace());
    }

    #[test]
    fn test_move_left_right() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.insert_char('🦀');
        input.insert_char('b');
        // cursor at end (6)
        input.move_left(); // back over 'b' → 5
        assert_eq!(input.cursor, 5);
        input.move_left(); // back over '🦀' → 1
        assert_eq!(input.cursor, 1);
        input.move_right(); // forward over '🦀' → 5
        assert_eq!(input.cursor, 5);
    }

    #[test]
    fn test_move_bounds() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.move_left();
        input.move_left(); // already at 0
        assert_eq!(input.cursor, 0);
        input.move_end();
        input.move_right(); // already at end
        assert_eq!(input.cursor, 1);
    }

    #[test]
    fn test_submit_and_history() {
        let mut input = InputState::new();
        input.insert_char('h');
        input.insert_char('i');
        let text = input.submit();
        assert_eq!(text, "hi");
        assert_eq!(input.buffer, "");
        assert_eq!(input.cursor, 0);
        assert_eq!(input.history.len(), 1);
    }

    #[test]
    fn test_history_navigation() {
        let mut input = InputState::new();
        input.buffer = "first".to_string();
        input.cursor = 5;
        input.submit();
        input.buffer = "second".to_string();
        input.cursor = 6;
        input.submit();

        input.history_prev();
        assert_eq!(input.buffer, "second");
        input.history_prev();
        assert_eq!(input.buffer, "first");
        input.history_next();
        assert_eq!(input.buffer, "second");
        input.history_next();
        assert_eq!(input.buffer, ""); // Past end
    }

    #[test]
    fn test_delete() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.insert_char('b');
        input.insert_char('c');
        input.move_home();
        assert!(input.delete()); // delete 'a'
        assert_eq!(input.buffer, "bc");
    }

    // Minimal InputState replica for testing
    struct InputState {
        buffer: String,
        cursor: usize,
        history: Vec<String>,
        history_index: Option<usize>,
    }

    impl InputState {
        fn new() -> Self {
            Self { buffer: String::new(), cursor: 0, history: Vec::new(), history_index: None }
        }

        fn insert_char(&mut self, c: char) {
            self.buffer.insert(self.cursor, c);
            self.cursor += c.len_utf8();
        }

        fn backspace(&mut self) -> bool {
            if self.cursor == 0 { return false; }
            let prev = self.buffer[..self.cursor]
                .char_indices().next_back().map(|(i, _)| i).unwrap_or(0);
            self.buffer.remove(prev);
            self.cursor = prev;
            true
        }

        fn delete(&mut self) -> bool {
            if self.cursor >= self.buffer.len() { return false; }
            self.buffer.remove(self.cursor);
            true
        }

        fn move_left(&mut self) {
            if self.cursor > 0 {
                self.cursor = self.buffer[..self.cursor]
                    .char_indices().next_back().map(|(i, _)| i).unwrap_or(0);
            }
        }

        fn move_right(&mut self) {
            if self.cursor < self.buffer.len() {
                self.cursor = self.buffer[self.cursor..]
                    .char_indices().nth(1).map(|(i, _)| self.cursor + i)
                    .unwrap_or(self.buffer.len());
            }
        }

        fn move_home(&mut self) { self.cursor = 0; }
        fn move_end(&mut self) { self.cursor = self.buffer.len(); }

        fn submit(&mut self) -> String {
            let text = self.buffer.trim().to_string();
            if !text.is_empty() { self.history.push(text.clone()); }
            self.buffer.clear();
            self.cursor = 0;
            self.history_index = None;
            text
        }

        fn history_prev(&mut self) {
            if self.history.is_empty() { return; }
            let idx = match self.history_index {
                Some(i) if i > 0 => i - 1,
                None if !self.history.is_empty() => self.history.len() - 1,
                _ => return,
            };
            self.history_index = Some(idx);
            self.buffer = self.history[idx].clone();
            self.cursor = self.buffer.len();
        }

        fn history_next(&mut self) {
            let idx = match self.history_index { Some(i) => i + 1, None => return };
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
}
