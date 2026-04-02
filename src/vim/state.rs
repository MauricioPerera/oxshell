use super::motions;

/// Vim editing modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VimMode {
    /// Normal mode — navigate and command
    Normal,
    /// Insert mode — typing text
    Insert,
}

/// Vim state machine for the input line
pub struct VimState {
    pub mode: VimMode,
    /// Pending operator (d, c, y)
    pub pending_op: Option<char>,
    /// Numeric count prefix
    pub count: Option<usize>,
    /// Last change for dot-repeat
    #[allow(dead_code)]
    pub last_change: Option<String>,
    /// Whether vim mode is enabled
    pub enabled: bool,
}

/// Result of processing a key in vim mode
pub enum VimAction {
    /// No action needed (key consumed internally)
    None,
    /// Insert this character at cursor
    #[allow(dead_code)]
    InsertChar(char),
    /// Delete characters (count)
    Delete(usize),
    /// Backspace (count)
    Backspace(usize),
    /// Move cursor by delta (negative = left)
    MoveCursor(i32),
    /// Move to absolute position
    MoveTo(CursorTarget),
    /// Switch to insert mode
    EnterInsert,
    /// Switch to normal mode
    EnterNormal,
    /// Delete to end of line and enter insert
    ChangeToEnd,
    /// Delete entire line content
    DeleteLine,
    /// Submit the line (like Enter)
    Submit,
    /// Pass through to TUI (key not handled by vim)
    PassThrough,
}

#[derive(Debug, Clone, Copy)]
pub enum CursorTarget {
    Start,      // 0
    FirstNonBlank, // ^
    End,        // $
    #[allow(dead_code)]
    WordForward,  // w
    #[allow(dead_code)]
    WordBack,     // b
    #[allow(dead_code)]
    WordEnd,      // e
}

impl VimState {
    pub fn new(enabled: bool) -> Self {
        Self {
            mode: if enabled { VimMode::Normal } else { VimMode::Insert },
            pending_op: None,
            count: None,
            last_change: None,
            enabled,
        }
    }

    /// Process a key press and return what action to take
    pub fn process_key(&mut self, c: char, buffer: &str, cursor: usize) -> VimAction {
        if !self.enabled {
            return VimAction::PassThrough;
        }

        match self.mode {
            VimMode::Insert => self.process_insert(c),
            VimMode::Normal => self.process_normal(c, buffer, cursor),
        }
    }

    fn process_insert(&mut self, c: char) -> VimAction {
        match c {
            '\x1b' => { // Escape
                self.mode = VimMode::Normal;
                VimAction::EnterNormal
            }
            _ => VimAction::PassThrough, // Let TUI handle normal typing
        }
    }

    fn process_normal(&mut self, c: char, buffer: &str, cursor: usize) -> VimAction {
        // Count prefix
        if c.is_ascii_digit() && c != '0' || (c == '0' && self.count.is_some()) {
            let digit = c.to_digit(10).unwrap() as usize;
            self.count = Some(self.count.unwrap_or(0) * 10 + digit);
            return VimAction::None;
        }

        let count = self.count.take().unwrap_or(1);

        // Pending operator (d, c, y)
        if let Some(op) = self.pending_op.take() {
            return self.execute_operator(op, c, count, buffer, cursor);
        }

        match c {
            // Mode switches
            'i' => {
                self.mode = VimMode::Insert;
                VimAction::EnterInsert
            }
            'a' => {
                self.mode = VimMode::Insert;
                VimAction::MoveCursor(1) // Move right then insert
            }
            'I' => {
                self.mode = VimMode::Insert;
                VimAction::MoveTo(CursorTarget::FirstNonBlank)
            }
            'A' => {
                self.mode = VimMode::Insert;
                VimAction::MoveTo(CursorTarget::End)
            }
            'o' | 'O' => {
                // In single-line input, o/O just enters insert at end
                self.mode = VimMode::Insert;
                VimAction::MoveTo(CursorTarget::End)
            }

            // Motions
            'h' => VimAction::MoveCursor(-(count as i32)),
            'l' => VimAction::MoveCursor(count as i32),
            'w' => {
                let pos = motions::word_forward(buffer, cursor, count);
                VimAction::MoveCursor(pos as i32 - cursor as i32)
            }
            'b' => {
                let pos = motions::word_back(buffer, cursor, count);
                VimAction::MoveCursor(pos as i32 - cursor as i32)
            }
            'e' => {
                let pos = motions::word_end(buffer, cursor, count);
                VimAction::MoveCursor(pos as i32 - cursor as i32)
            }
            '0' => VimAction::MoveTo(CursorTarget::Start),
            '^' => VimAction::MoveTo(CursorTarget::FirstNonBlank),
            '$' => VimAction::MoveTo(CursorTarget::End),

            // Operators (wait for motion)
            'd' => {
                self.pending_op = Some('d');
                VimAction::None
            }
            'c' => {
                self.pending_op = Some('c');
                VimAction::None
            }

            // Quick operations
            'x' => VimAction::Delete(count),
            'D' => VimAction::ChangeToEnd,
            'C' => {
                self.mode = VimMode::Insert;
                VimAction::ChangeToEnd
            }
            #[allow(unreachable_patterns)]
            'S' | 'c' if self.pending_op == Some('c') => {
                self.mode = VimMode::Insert;
                VimAction::DeleteLine
            }

            // Submit
            '\n' | '\r' => VimAction::Submit,

            // Escape clears pending
            '\x1b' => {
                self.pending_op = None;
                self.count = None;
                VimAction::None
            }

            _ => VimAction::None,
        }
    }

    fn execute_operator(
        &mut self,
        op: char,
        motion: char,
        count: usize,
        buffer: &str,
        cursor: usize,
    ) -> VimAction {
        match (op, motion) {
            // dd = delete line
            ('d', 'd') => VimAction::DeleteLine,
            // cc = change line
            ('c', 'c') => {
                self.mode = VimMode::Insert;
                VimAction::DeleteLine
            }
            // dw = delete word
            ('d', 'w') => {
                let target = motions::word_forward(buffer, cursor, count);
                let chars_to_delete = target.saturating_sub(cursor);
                VimAction::Delete(chars_to_delete)
            }
            // db = delete back word
            ('d', 'b') => {
                let target = motions::word_back(buffer, cursor, count);
                let chars_to_delete = cursor.saturating_sub(target);
                VimAction::Backspace(chars_to_delete)
            }
            // cw = change word
            ('c', 'w') => {
                self.mode = VimMode::Insert;
                let target = motions::word_forward(buffer, cursor, count);
                let chars_to_delete = target.saturating_sub(cursor);
                VimAction::Delete(chars_to_delete)
            }
            // d$ = delete to end
            ('d', '$') => VimAction::ChangeToEnd,
            // c$ = change to end
            ('c', '$') => {
                self.mode = VimMode::Insert;
                VimAction::ChangeToEnd
            }
            _ => VimAction::None,
        }
    }
}
