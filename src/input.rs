use zeroize::Zeroizing;

/// Handles keyboard input for password entry
pub struct InputHandler {
    password_buffer: Zeroizing<String>,
    cursor_position: usize,
    wrong_password_timer: Option<std::time::Instant>,
    key_highlight_timer: Option<std::time::Instant>,
    caps_lock: bool,
    config: crate::config::Config,
    last_failed_attempt: Option<std::time::Instant>,
}

impl InputHandler {
    pub fn new(config: crate::config::Config) -> Self {
        Self {
            password_buffer: Zeroizing::new(String::new()),
            cursor_position: 0,
            wrong_password_timer: None,
            key_highlight_timer: None,
            caps_lock: false,
            config,
            last_failed_attempt: None,
        }
    }

    /// Handle a key event from Wayland
    pub fn handle_key_event(
        &mut self,
        keysym: smithay_client_toolkit::seat::keyboard::Keysym,
        utf8: Option<String>,
        modifiers: smithay_client_toolkit::seat::keyboard::Modifiers,
    ) -> InputAction {
        if self.is_cooldown() {
            return InputAction::None;
        }

        self.caps_lock = modifiers.caps_lock;

        if modifiers.ctrl && keysym == Keysym::u {
            if !self.password_buffer.is_empty() {
                self.password_buffer.clear();
                self.cursor_position = 0;
                return InputAction::PasswordCleared;
            }
            return InputAction::None;
        }

        // Handle special keys first using keysym
        use smithay_client_toolkit::seat::keyboard::Keysym;
        match keysym {
            Keysym::BackSpace => {
                if self.password_buffer.is_empty() || self.cursor_position == 0 {
                    return InputAction::None;
                }
                self.cursor_position -= 1;
                self.password_buffer.remove(self.cursor_position);
                if self.password_buffer.is_empty() {
                    return InputAction::PasswordCleared;
                }
                return InputAction::PasswordChanged;
            }
            Keysym::Return | Keysym::KP_Enter => {
                let password = self.password_buffer.clone();
                self.password_buffer.clear();
                self.cursor_position = 0;
                return InputAction::SubmitPassword(password);
            }
            Keysym::Escape => {
                return InputAction::Cancel;
            }
            Keysym::Left => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    return InputAction::CursorMoved;
                }
                return InputAction::None;
            }
            Keysym::Right => {
                if self.cursor_position < self.password_buffer.len() {
                    self.cursor_position += 1;
                    return InputAction::CursorMoved;
                }
                return InputAction::None;
            }
            Keysym::Home => {
                if self.cursor_position > 0 {
                    self.cursor_position = 0;
                    return InputAction::CursorMoved;
                }
                return InputAction::None;
            }
            Keysym::End => {
                if self.cursor_position < self.password_buffer.len() {
                    self.cursor_position = self.password_buffer.len();
                    return InputAction::CursorMoved;
                }
                return InputAction::None;
            }
            Keysym::Delete => {
                if self.cursor_position < self.password_buffer.len() {
                    self.password_buffer.remove(self.cursor_position);
                    if self.password_buffer.is_empty() {
                        return InputAction::PasswordCleared;
                    }
                    return InputAction::PasswordChanged;
                }
                return InputAction::None;
            }
            _ => {}
        }

        // Use the UTF-8 string provided by SCTK for character input
        if let Some(txt) = utf8 {
            for c in txt.chars() {
                if c.is_ascii() && !c.is_control() {
                    self.password_buffer.insert(self.cursor_position, c);
                    self.cursor_position += 1;
                }
            }
            return InputAction::PasswordChanged;
        }

        InputAction::None
    }

    pub fn password_buffer(&self) -> &Zeroizing<String> {
        &self.password_buffer
    }

    pub fn password_length(&self) -> usize {
        self.password_buffer.len()
    }

    pub fn cursor_position(&self) -> usize {
        self.cursor_position
    }

    /// Set wrong password feedback timer
    pub fn set_wrong_password_feedback(&mut self) {
        self.wrong_password_timer = Some(std::time::Instant::now());
        self.last_failed_attempt = Some(std::time::Instant::now());
    }

    pub fn is_cooldown(&self) -> bool {
        self.last_failed_attempt
            .map(|t| t.elapsed() < std::time::Duration::from_millis(400))
            .unwrap_or(false)
    }

    /// Check if wrong password feedback should be shown
    pub fn should_show_wrong_password(&self) -> bool {
        if let Some(timer) = self.wrong_password_timer {
            timer.elapsed() < std::time::Duration::from_millis(self.config.feedback_window_duration)
        } else {
            false
        }
    }

    /// Set key highlight timer (for visual feedback)
    pub fn set_key_highlight(&mut self) {
        self.key_highlight_timer = Some(std::time::Instant::now());
    }

    /// Check if key highlight should be shown
    pub fn should_show_key_highlight(&self) -> bool {
        if let Some(timer) = self.key_highlight_timer {
            timer.elapsed() < std::time::Duration::from_millis(self.config.key_highlight_window_duration)
        } else {
            false
        }
    }

    /// Get the current Caps Lock state
    pub fn caps_lock(&self) -> bool {
        self.caps_lock
    }
}

/// Actions that can result from keyboard input
#[derive(Debug)]
pub enum InputAction {
    None,
    PasswordChanged,
    PasswordCleared,
    CursorMoved,
    SubmitPassword(Zeroizing<String>),
    Cancel,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use clap::Parser;
    use smithay_client_toolkit::seat::keyboard::{Keysym, Modifiers};

    fn test_config() -> Config {
        Config::parse_from(["test"])
    }

    #[test]
    fn test_new_handler_defaults() {
        let handler = InputHandler::new(test_config());
        assert_eq!(handler.password_length(), 0);
        assert_eq!(handler.cursor_position(), 0);
        assert!(!handler.caps_lock());
        assert!(!handler.should_show_wrong_password());
        assert!(!handler.should_show_key_highlight());
    }

    #[test]
    fn test_character_input_appends() {
        let mut handler = InputHandler::new(test_config());
        let mods = Modifiers::default();

        let action = handler.handle_key_event(Keysym::a, Some("a".to_string()), mods);
        assert!(matches!(action, InputAction::PasswordChanged));
        assert_eq!(handler.password_length(), 1);
        assert_eq!(handler.cursor_position(), 1);

        let action = handler.handle_key_event(Keysym::b, Some("b".to_string()), mods);
        assert!(matches!(action, InputAction::PasswordChanged));
        assert_eq!(handler.password_length(), 2);
        assert_eq!(handler.cursor_position(), 2);

        assert_eq!(&*handler.password_buffer, "ab");
    }

    #[test]
    fn test_backspace_removes_last_char() {
        let mut handler = InputHandler::new(test_config());
        let mods = Modifiers::default();

        handler.handle_key_event(Keysym::a, Some("a".to_string()), mods);
        handler.handle_key_event(Keysym::b, Some("b".to_string()), mods);
        assert_eq!(handler.password_length(), 2);

        let action = handler.handle_key_event(Keysym::BackSpace, None, mods);
        assert!(matches!(action, InputAction::PasswordChanged));
        assert_eq!(handler.password_length(), 1);
        assert_eq!(handler.cursor_position(), 1);
        assert_eq!(&*handler.password_buffer, "a");
    }

    #[test]
    fn test_backspace_on_empty_buffer() {
        let mut handler = InputHandler::new(test_config());
        let mods = Modifiers::default();

        let action = handler.handle_key_event(Keysym::BackSpace, None, mods);
        assert!(matches!(action, InputAction::None));
        assert_eq!(handler.password_length(), 0);
    }

    #[test]
    fn test_backspace_last_char_clears() {
        let mut handler = InputHandler::new(test_config());
        let mods = Modifiers::default();

        handler.handle_key_event(Keysym::a, Some("a".to_string()), mods);
        let action = handler.handle_key_event(Keysym::BackSpace, None, mods);
        assert!(matches!(action, InputAction::PasswordCleared));
        assert_eq!(handler.password_length(), 0);
    }

    #[test]
    fn test_ctrl_u_clears_buffer() {
        let mut handler = InputHandler::new(test_config());
        let mods = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };

        handler.handle_key_event(Keysym::a, Some("a".to_string()), Modifiers::default());
        handler.handle_key_event(Keysym::b, Some("b".to_string()), Modifiers::default());
        assert_eq!(handler.password_length(), 2);

        let action = handler.handle_key_event(Keysym::u, None, mods);
        assert!(matches!(action, InputAction::PasswordCleared));
        assert_eq!(handler.password_length(), 0);
        assert_eq!(handler.cursor_position(), 0);
    }

    #[test]
    fn test_submit_returns_and_clears() {
        let mut handler = InputHandler::new(test_config());
        let mods = Modifiers::default();

        handler.handle_key_event(Keysym::a, Some("a".to_string()), mods);
        handler.handle_key_event(Keysym::b, Some("b".to_string()), mods);

        let action = handler.handle_key_event(Keysym::Return, None, mods);
        match action {
            InputAction::SubmitPassword(p) => {
                assert_eq!(&*p, "ab");
            }
            _ => panic!("Expected SubmitPassword, got {:?}", action),
        }
        // Buffer should be cleared after submission
        assert_eq!(handler.password_length(), 0);
        assert_eq!(handler.cursor_position(), 0);
    }

    #[test]
    fn test_submit_enter_kp() {
        let mut handler = InputHandler::new(test_config());
        let mods = Modifiers::default();
        handler.handle_key_event(Keysym::a, Some("a".to_string()), mods);

        let action = handler.handle_key_event(Keysym::KP_Enter, None, Modifiers::default());
        assert!(matches!(action, InputAction::SubmitPassword(_)));
    }

    #[test]
    fn test_escape_cancels() {
        let mut handler = InputHandler::new(test_config());
        let mods = Modifiers::default();

        let action = handler.handle_key_event(Keysym::Escape, None, mods);
        assert!(matches!(action, InputAction::Cancel));
    }

    #[test]
    fn test_cursor_left_right() {
        let mut handler = InputHandler::new(test_config());
        let mods = Modifiers::default();

        handler.handle_key_event(Keysym::a, Some("a".to_string()), mods);
        handler.handle_key_event(Keysym::b, Some("b".to_string()), mods);
        handler.handle_key_event(Keysym::c, Some("c".to_string()), mods);
        assert_eq!(handler.cursor_position(), 3);

        // Move left
        let action = handler.handle_key_event(Keysym::Left, None, mods);
        assert!(matches!(action, InputAction::CursorMoved));
        assert_eq!(handler.cursor_position(), 2);

        // Left again
        handler.handle_key_event(Keysym::Left, None, mods);
        assert_eq!(handler.cursor_position(), 1);

        // Right
        let action = handler.handle_key_event(Keysym::Right, None, mods);
        assert!(matches!(action, InputAction::CursorMoved));
        assert_eq!(handler.cursor_position(), 2);
    }

    #[test]
    fn test_cursor_left_at_start() {
        let mut handler = InputHandler::new(test_config());
        let mods = Modifiers::default();

        let action = handler.handle_key_event(Keysym::Left, None, mods);
        assert!(matches!(action, InputAction::None));
        assert_eq!(handler.cursor_position(), 0);
    }

    #[test]
    fn test_cursor_right_at_end() {
        let mut handler = InputHandler::new(test_config());
        let mods = Modifiers::default();
        handler.handle_key_event(Keysym::a, Some("a".to_string()), mods);

        let action = handler.handle_key_event(Keysym::Right, None, mods);
        assert!(matches!(action, InputAction::None));
        assert_eq!(handler.cursor_position(), 1);
    }

    #[test]
    fn test_home_and_end() {
        let mut handler = InputHandler::new(test_config());
        let mods = Modifiers::default();
        handler.handle_key_event(Keysym::a, Some("a".to_string()), mods);
        handler.handle_key_event(Keysym::b, Some("b".to_string()), mods);
        handler.handle_key_event(Keysym::c, Some("c".to_string()), mods);
        handler.handle_key_event(Keysym::Left, None, mods);
        handler.handle_key_event(Keysym::Left, None, mods);
        assert_eq!(handler.cursor_position(), 1);

        // Home
        let action = handler.handle_key_event(Keysym::Home, None, mods);
        assert!(matches!(action, InputAction::CursorMoved));
        assert_eq!(handler.cursor_position(), 0);

        // End
        let action = handler.handle_key_event(Keysym::End, None, mods);
        assert!(matches!(action, InputAction::CursorMoved));
        assert_eq!(handler.cursor_position(), 3);
    }

    #[test]
    fn test_delete_removes_at_cursor() {
        let mut handler = InputHandler::new(test_config());
        let mods = Modifiers::default();
        handler.handle_key_event(Keysym::a, Some("a".to_string()), mods);
        handler.handle_key_event(Keysym::b, Some("b".to_string()), mods);
        handler.handle_key_event(Keysym::c, Some("c".to_string()), mods);
        // cursor at 3, delete should be a no-op
        let action = handler.handle_key_event(Keysym::Delete, None, mods);
        assert!(matches!(action, InputAction::None));
        assert_eq!(handler.password_length(), 3);

        // move left, delete at cursor position 2 (removes 'c')
        handler.handle_key_event(Keysym::Left, None, mods);
        let action = handler.handle_key_event(Keysym::Delete, None, mods);
        assert!(matches!(action, InputAction::PasswordChanged));
        assert_eq!(handler.password_length(), 2);
        assert_eq!(&*handler.password_buffer, "ab");
    }

    #[test]
    fn test_insert_mid_buffer() {
        let mut handler = InputHandler::new(test_config());
        let mods = Modifiers::default();
        handler.handle_key_event(Keysym::a, Some("a".to_string()), mods);
        handler.handle_key_event(Keysym::c, Some("c".to_string()), mods);
        // Move left, insert 'b' between a and c
        handler.handle_key_event(Keysym::Left, None, mods);
        handler.handle_key_event(Keysym::b, Some("b".to_string()), mods);
        assert_eq!(&*handler.password_buffer, "abc");
        assert_eq!(handler.cursor_position(), 2);
    }

    #[test]
    fn test_caps_lock_tracking() {
        let mut handler = InputHandler::new(test_config());
        assert!(!handler.caps_lock());

        let caps_mods = Modifiers {
            caps_lock: true,
            ..Modifiers::default()
        };
        handler.handle_key_event(Keysym::a, Some("A".to_string()), caps_mods);
        assert!(handler.caps_lock());
    }

    #[test]
    fn test_wrong_password_timer() {
        let mut handler = InputHandler::new(test_config());
        assert!(!handler.should_show_wrong_password());

        handler.set_wrong_password_feedback();
        assert!(handler.should_show_wrong_password());
    }

    #[test]
    fn test_key_highlight_timer() {
        let mut handler = InputHandler::new(test_config());
        assert!(!handler.should_show_key_highlight());

        handler.set_key_highlight();
        assert!(handler.should_show_key_highlight());
    }
}
