use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct KeyBinding {
    pub(crate) code: KeyCode,
    pub(crate) modifiers: KeyModifiers,
}

impl KeyBinding {
    pub(crate) fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub(crate) fn matches(&self, event: &KeyEvent) -> bool {
        self.code == event.code && self.modifiers == event.modifiers
    }
}

pub(crate) fn plain(code: KeyCode) -> KeyBinding {
    KeyBinding::new(code, KeyModifiers::NONE)
}

pub(crate) fn ctrl(code: KeyCode) -> KeyBinding {
    KeyBinding::new(code, KeyModifiers::CONTROL)
}

pub(crate) fn alt(code: KeyCode) -> KeyBinding {
    KeyBinding::new(code, KeyModifiers::ALT)
}

pub(crate) fn shift(code: KeyCode) -> KeyBinding {
    KeyBinding::new(code, KeyModifiers::SHIFT)
}

#[derive(Debug, Clone)]
pub(crate) struct AppKeymap {
    pub(crate) open_transcript: Vec<KeyBinding>,
    pub(crate) open_external_editor: Vec<KeyBinding>,
    pub(crate) copy: Vec<KeyBinding>,
    pub(crate) clear_terminal: Vec<KeyBinding>,
    pub(crate) toggle_vim_mode: Vec<KeyBinding>,
}

#[derive(Debug, Clone)]
pub(crate) struct ChatKeymap {
    pub(crate) interrupt_turn: Vec<KeyBinding>,
    pub(crate) decrease_reasoning_effort: Vec<KeyBinding>,
    pub(crate) increase_reasoning_effort: Vec<KeyBinding>,
}

#[derive(Debug, Clone)]
pub(crate) struct ComposerKeymap {
    pub(crate) submit: Vec<KeyBinding>,
    pub(crate) queue: Vec<KeyBinding>,
    pub(crate) toggle_shortcuts: Vec<KeyBinding>,
    pub(crate) history_search_previous: Vec<KeyBinding>,
    pub(crate) history_search_next: Vec<KeyBinding>,
}

#[derive(Debug, Clone)]
pub(crate) struct EditorKeymap {
    pub(crate) insert_newline: Vec<KeyBinding>,
    pub(crate) move_left: Vec<KeyBinding>,
    pub(crate) move_right: Vec<KeyBinding>,
    pub(crate) move_up: Vec<KeyBinding>,
    pub(crate) move_down: Vec<KeyBinding>,
    pub(crate) move_word_left: Vec<KeyBinding>,
    pub(crate) move_word_right: Vec<KeyBinding>,
    pub(crate) move_line_start: Vec<KeyBinding>,
    pub(crate) move_line_end: Vec<KeyBinding>,
    pub(crate) delete_backward: Vec<KeyBinding>,
    pub(crate) delete_forward: Vec<KeyBinding>,
    pub(crate) delete_backward_word: Vec<KeyBinding>,
    pub(crate) delete_forward_word: Vec<KeyBinding>,
    pub(crate) kill_line_start: Vec<KeyBinding>,
    pub(crate) kill_whole_line: Vec<KeyBinding>,
    pub(crate) kill_line_end: Vec<KeyBinding>,
    pub(crate) yank: Vec<KeyBinding>,
}

#[derive(Debug, Clone)]
pub(crate) struct PagerKeymap {
    pub(crate) scroll_up: Vec<KeyBinding>,
    pub(crate) scroll_down: Vec<KeyBinding>,
    pub(crate) page_up: Vec<KeyBinding>,
    pub(crate) page_down: Vec<KeyBinding>,
    pub(crate) jump_top: Vec<KeyBinding>,
    pub(crate) jump_bottom: Vec<KeyBinding>,
    pub(crate) close: Vec<KeyBinding>,
}

#[derive(Debug, Clone)]
pub(crate) struct ApprovalKeymap {
    pub(crate) approve: Vec<KeyBinding>,
    pub(crate) approve_session: Vec<KeyBinding>,
    pub(crate) deny: Vec<KeyBinding>,
    pub(crate) cancel: Vec<KeyBinding>,
}

#[derive(Debug, Clone)]
pub(crate) struct ListKeymap {
    pub(crate) move_up: Vec<KeyBinding>,
    pub(crate) move_down: Vec<KeyBinding>,
    pub(crate) accept: Vec<KeyBinding>,
    pub(crate) cancel: Vec<KeyBinding>,
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeKeymap {
    pub(crate) app: AppKeymap,
    pub(crate) chat: ChatKeymap,
    pub(crate) composer: ComposerKeymap,
    pub(crate) editor: EditorKeymap,
    pub(crate) pager: PagerKeymap,
    pub(crate) list: ListKeymap,
    pub(crate) approval: ApprovalKeymap,
}

impl RuntimeKeymap {
    pub(crate) fn defaults() -> Self {
        Self {
            app: AppKeymap {
                open_transcript: vec![ctrl(KeyCode::Char('t'))],
                open_external_editor: vec![ctrl(KeyCode::Char('g'))],
                copy: vec![ctrl(KeyCode::Char('o'))],
                clear_terminal: vec![ctrl(KeyCode::Char('l'))],
                toggle_vim_mode: vec![],
            },
            chat: ChatKeymap {
                interrupt_turn: vec![ctrl(KeyCode::Char('c'))],
                decrease_reasoning_effort: vec![alt(KeyCode::Char(','))],
                increase_reasoning_effort: vec![alt(KeyCode::Char('.'))],
            },
            composer: ComposerKeymap {
                submit: vec![plain(KeyCode::Enter), plain(KeyCode::Char('\r')), plain(KeyCode::Char('\n'))],
                queue: vec![plain(KeyCode::Tab)],
                toggle_shortcuts: vec![plain(KeyCode::Char('?')), shift(KeyCode::Char('?'))],
                history_search_previous: vec![ctrl(KeyCode::Char('r'))],
                history_search_next: vec![ctrl(KeyCode::Char('s'))],
            },
            editor: EditorKeymap {
                insert_newline: vec![ctrl(KeyCode::Char('j')), ctrl(KeyCode::Char('m')), alt(KeyCode::Enter)],
                move_left: vec![plain(KeyCode::Left)],
                move_right: vec![plain(KeyCode::Right)],
                move_up: vec![plain(KeyCode::Up)],
                move_down: vec![plain(KeyCode::Down)],
                move_word_left: vec![alt(KeyCode::Char('b')), alt(KeyCode::Left), ctrl(KeyCode::Left)],
                move_word_right: vec![alt(KeyCode::Char('f')), alt(KeyCode::Right), ctrl(KeyCode::Right)],
                move_line_start: vec![plain(KeyCode::Home)],
                move_line_end: vec![plain(KeyCode::End)],
                delete_backward: vec![plain(KeyCode::Backspace)],
                delete_forward: vec![plain(KeyCode::Delete)],
                delete_backward_word: vec![ctrl(KeyCode::Char('w')), alt(KeyCode::Backspace), ctrl(KeyCode::Backspace)],
                delete_forward_word: vec![alt(KeyCode::Delete), ctrl(KeyCode::Delete)],
                kill_line_start: vec![ctrl(KeyCode::Char('u'))],
                kill_whole_line: vec![],
                kill_line_end: vec![ctrl(KeyCode::Char('k'))],
                yank: vec![ctrl(KeyCode::Char('y'))],
            },
            pager: PagerKeymap {
                scroll_up: vec![plain(KeyCode::Up), plain(KeyCode::Char('k'))],
                scroll_down: vec![plain(KeyCode::Down), plain(KeyCode::Char('j'))],
                page_up: vec![plain(KeyCode::PageUp)],
                page_down: vec![plain(KeyCode::PageDown)],
                jump_top: vec![plain(KeyCode::Char('g'))],
                jump_bottom: vec![plain(KeyCode::Char('G'))],
                close: vec![plain(KeyCode::Char('q')), plain(KeyCode::Esc), ctrl(KeyCode::Char('c'))],
            },
            approval: ApprovalKeymap {
                approve: vec![plain(KeyCode::Enter), plain(KeyCode::Char('a'))],
                approve_session: vec![plain(KeyCode::Char('s'))],
                deny: vec![plain(KeyCode::Char('d')), plain(KeyCode::Esc)],
                cancel: vec![ctrl(KeyCode::Char('c'))],
            },
            list: ListKeymap {
                move_up: vec![plain(KeyCode::Up), plain(KeyCode::Char('k'))],
                move_down: vec![plain(KeyCode::Down), plain(KeyCode::Char('j'))],
                accept: vec![plain(KeyCode::Enter)],
                cancel: vec![plain(KeyCode::Esc)],
            },
        }
    }

    fn find_match(bindings: &[KeyBinding], event: &KeyEvent) -> bool {
        bindings.iter().any(|b| b.matches(event))
    }

    pub(crate) fn match_app_action(&self, event: &KeyEvent) -> Option<&'static str> {
        if Self::find_match(&self.app.clear_terminal, event) { return Some("clear_terminal"); }
        if Self::find_match(&self.app.open_transcript, event) { return Some("open_transcript"); }
        if Self::find_match(&self.app.copy, event) { return Some("copy"); }
        if Self::find_match(&self.app.open_external_editor, event) { return Some("open_external_editor"); }
        None
    }

    pub(crate) fn match_chat_action(&self, event: &KeyEvent) -> Option<&'static str> {
        if Self::find_match(&self.chat.interrupt_turn, event) { return Some("interrupt_turn"); }
        if Self::find_match(&self.chat.decrease_reasoning_effort, event) { return Some("decrease_reasoning_effort"); }
        if Self::find_match(&self.chat.increase_reasoning_effort, event) { return Some("increase_reasoning_effort"); }
        None
    }

    pub(crate) fn match_composer_action(&self, event: &KeyEvent) -> Option<&'static str> {
        if Self::find_match(&self.composer.submit, event) { return Some("submit"); }
        if Self::find_match(&self.composer.queue, event) { return Some("queue"); }
        if Self::find_match(&self.composer.toggle_shortcuts, event) { return Some("toggle_shortcuts"); }
        if Self::find_match(&self.composer.history_search_previous, event) { return Some("history_search_previous"); }
        if Self::find_match(&self.composer.history_search_next, event) { return Some("history_search_next"); }
        None
    }

    pub(crate) fn match_editor_action(&self, event: &KeyEvent) -> Option<&'static str> {
        if Self::find_match(&self.editor.insert_newline, event) { return Some("insert_newline"); }
        if Self::find_match(&self.editor.move_left, event) { return Some("move_left"); }
        if Self::find_match(&self.editor.move_right, event) { return Some("move_right"); }
        if Self::find_match(&self.editor.move_up, event) { return Some("move_up"); }
        if Self::find_match(&self.editor.move_down, event) { return Some("move_down"); }
        if Self::find_match(&self.editor.move_word_left, event) { return Some("move_word_left"); }
        if Self::find_match(&self.editor.move_word_right, event) { return Some("move_word_right"); }
        if Self::find_match(&self.editor.move_line_start, event) { return Some("move_line_start"); }
        if Self::find_match(&self.editor.move_line_end, event) { return Some("move_line_end"); }
        if Self::find_match(&self.editor.delete_backward, event) { return Some("delete_backward"); }
        if Self::find_match(&self.editor.delete_forward, event) { return Some("delete_forward"); }
        if Self::find_match(&self.editor.delete_backward_word, event) { return Some("delete_backward_word"); }
        if Self::find_match(&self.editor.delete_forward_word, event) { return Some("delete_forward_word"); }
        if Self::find_match(&self.editor.kill_line_start, event) { return Some("kill_line_start"); }
        if Self::find_match(&self.editor.kill_whole_line, event) { return Some("kill_whole_line"); }
        if Self::find_match(&self.editor.kill_line_end, event) { return Some("kill_line_end"); }
        if Self::find_match(&self.editor.yank, event) { return Some("yank"); }
        None
    }

    pub(crate) fn match_pager_action(&self, event: &KeyEvent) -> Option<&'static str> {
        if Self::find_match(&self.pager.scroll_up, event) { return Some("scroll_up"); }
        if Self::find_match(&self.pager.scroll_down, event) { return Some("scroll_down"); }
        if Self::find_match(&self.pager.page_up, event) { return Some("page_up"); }
        if Self::find_match(&self.pager.page_down, event) { return Some("page_down"); }
        if Self::find_match(&self.pager.jump_top, event) { return Some("jump_top"); }
        if Self::find_match(&self.pager.jump_bottom, event) { return Some("jump_bottom"); }
        if Self::find_match(&self.pager.close, event) { return Some("close"); }
        None
    }

    pub(crate) fn match_approval_action(&self, event: &KeyEvent) -> Option<&'static str> {
        if Self::find_match(&self.approval.approve, event) { return Some("approve"); }
        if Self::find_match(&self.approval.approve_session, event) { return Some("approve_session"); }
        if Self::find_match(&self.approval.deny, event) { return Some("deny"); }
        if Self::find_match(&self.approval.cancel, event) { return Some("cancel"); }
        None
    }

    fn check_conflicts(label: &str, bindings: &[Vec<KeyBinding>]) -> Result<(), String> {
        let mut seen = HashSet::new();
        for (_action_idx, action_bindings) in bindings.iter().enumerate() {
            for binding in action_bindings {
                if !seen.insert(*binding) {
                    return Err(format!("{label}: duplicate key binding {:?}", binding));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        Self::check_conflicts("app", &[
            self.app.open_transcript.clone(),
            self.app.open_external_editor.clone(),
            self.app.copy.clone(),
            self.app.clear_terminal.clone(),
        ])?;
        Self::check_conflicts("editor", &[
            self.editor.move_left.clone(),
            self.editor.move_right.clone(),
            self.editor.move_up.clone(),
            self.editor.move_down.clone(),
            self.editor.move_word_left.clone(),
            self.editor.move_word_right.clone(),
            self.editor.delete_backward.clone(),
            self.editor.delete_forward.clone(),
            self.editor.delete_backward_word.clone(),
            self.editor.delete_forward_word.clone(),
            self.editor.kill_line_start.clone(),
            self.editor.kill_line_end.clone(),
            self.editor.yank.clone(),
        ])?;
        Self::check_conflicts("composer", &[
            self.composer.submit.clone(),
            self.composer.queue.clone(),
            self.composer.history_search_previous.clone(),
            self.composer.history_search_next.clone(),
        ])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_keymap_has_submit_bindings() {
        let km = RuntimeKeymap::defaults();
        assert!(!km.composer.submit.is_empty());
    }

    #[test]
    fn default_keymap_has_movement_bindings() {
        let km = RuntimeKeymap::defaults();
        assert!(!km.editor.move_left.is_empty());
        assert!(!km.editor.move_right.is_empty());
    }

    #[test]
    fn validation_passes_for_defaults() {
        let km = RuntimeKeymap::defaults();
        assert!(km.validate().is_ok());
    }

    #[test]
    fn app_action_matches_clear_terminal() {
        let km = RuntimeKeymap::defaults();
        let event = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL);
        assert_eq!(km.match_app_action(&event), Some("clear_terminal"));
    }

    #[test]
    fn composer_action_matches_submit() {
        let km = RuntimeKeymap::defaults();
        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(km.match_composer_action(&event), Some("submit"));
    }

    #[test]
    fn editor_action_matches_move_left() {
        let km = RuntimeKeymap::defaults();
        let event = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(km.match_editor_action(&event), Some("move_left"));
    }
}