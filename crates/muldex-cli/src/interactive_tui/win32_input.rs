use std::io;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Console::{
    INPUT_RECORD, KEY_EVENT_RECORD, GetNumberOfConsoleInputEvents, GetStdHandle,
    ReadConsoleInputW, STD_INPUT_HANDLE,
};

// Console input-event types (INPUT_RECORD.EventType).
const KEY_EVENT_TYPE: u16 = 0x0001;
const WINDOW_BUFFER_SIZE_EVENT_TYPE: u16 = 0x0004;

// Console control-key-state flags (dwControlKeyState).
const RIGHT_ALT_PRESSED: u32 = 0x0001;
const LEFT_ALT_PRESSED: u32 = 0x0002;
const RIGHT_CTRL_PRESSED: u32 = 0x0004;
const LEFT_CTRL_PRESSED: u32 = 0x0008;
const SHIFT_PRESSED: u32 = 0x0010;

/// A single translated console input event.
pub(crate) enum ConsoleInput {
    Key(KeyEvent),
    Resize,
    None,
}

/// Returns true if any console input is pending (non-blocking).
pub(crate) fn poll_console_input() -> io::Result<bool> {
    let input = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    if input == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let mut count = 0u32;
    if unsafe { GetNumberOfConsoleInputEvents(input, &mut count) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(count > 0)
}

/// Read the next queued console input record and translate it. Returns
/// `ConsoleInput::None` for key-up, mouse or focus events (the caller should
/// keep reading); `ConsoleInput::Resize` for a window-buffer size change.
pub(crate) fn read_console_input() -> io::Result<ConsoleInput> {
    let input = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    if input == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let mut record = unsafe { std::mem::zeroed::<INPUT_RECORD>() };
    let mut read = 0u32;
    if unsafe { ReadConsoleInputW(input, &mut record, 1, &mut read) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if read == 0 {
        return Ok(ConsoleInput::None);
    }
    match record.EventType {
        KEY_EVENT_TYPE => {
            let key = unsafe { record.Event.KeyEvent };
            if key.bKeyDown == 0 {
                Ok(ConsoleInput::None)
            } else {
                Ok(ConsoleInput::Key(translate_key_event(&key)))
            }
        }
        WINDOW_BUFFER_SIZE_EVENT_TYPE => Ok(ConsoleInput::Resize),
        _ => Ok(ConsoleInput::None),
    }
}

fn translate_key_event(record: &KEY_EVENT_RECORD) -> KeyEvent {
    let state = record.dwControlKeyState;
    let mut modifiers = KeyModifiers::empty();
    if state & (RIGHT_ALT_PRESSED | LEFT_ALT_PRESSED) != 0 {
        modifiers |= KeyModifiers::ALT;
    }
    if state & (RIGHT_CTRL_PRESSED | LEFT_CTRL_PRESSED) != 0 {
        modifiers |= KeyModifiers::CONTROL;
    }
    if state & SHIFT_PRESSED != 0 {
        modifiers |= KeyModifiers::SHIFT;
    }

    // The actual typed character (respects Shift/CAPS/IME); fall back to the
    // virtual-key code for keys that do not produce a character.
    let ch = unsafe { record.uChar.UnicodeChar };
    let mut code = if ch != 0 {
        match char::from_u32(ch as u32) {
            Some(c) => KeyCode::Char(c),
            None => KeyCode::Null,
        }
    } else {
        translate_vk(record.wVirtualKeyCode)
    };

    // Mirror crossterm's Ctrl+letter normalization so existing keymaps (which
    // match `Char('a') | CONTROL`) keep working: a C0 control code (1..=26)
    // produced while Ctrl is held becomes the corresponding letter.
    if modifiers.contains(KeyModifiers::CONTROL) {
        if let KeyCode::Char(c) = code {
            let cp = c as u32;
            if (1..=26).contains(&cp) {
                code = KeyCode::Char((b'a' + (cp as u8 - 1)) as char);
            }
        }
    }

    KeyEvent::new(code, modifiers)
}

fn translate_vk(vk: u16) -> KeyCode {
    match vk {
        0x08 => KeyCode::Backspace,
        0x09 => KeyCode::Tab,
        0x0D => KeyCode::Enter,
        0x1B => KeyCode::Esc,
        0x21 => KeyCode::PageUp,
        0x22 => KeyCode::PageDown,
        0x23 => KeyCode::End,
        0x24 => KeyCode::Home,
        0x25 => KeyCode::Left,
        0x26 => KeyCode::Up,
        0x27 => KeyCode::Right,
        0x28 => KeyCode::Down,
        0x2D => KeyCode::Insert,
        0x2E => KeyCode::Delete,
        0x30..=0x39 => KeyCode::Char((b'0' + (vk - 0x30) as u8) as char),
        0x41..=0x5A => KeyCode::Char((b'A' + (vk - 0x41) as u8) as char),
        0x70..=0x7B => KeyCode::F((vk - 0x6F) as u8),
        _ => KeyCode::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_record(vk: u16, ch: u16, state: u32) -> KEY_EVENT_RECORD {
        let mut record = KEY_EVENT_RECORD {
            bKeyDown: 1,
            wRepeatCount: 1,
            wVirtualKeyCode: vk,
            wVirtualScanCode: 0,
            uChar: unsafe { std::mem::zeroed::<_>() },
            dwControlKeyState: state,
        };
        unsafe {
            record.uChar.UnicodeChar = ch;
        }
        record
    }

    #[test]
    fn translates_printable_character() {
        let record = key_record(0x41, b'a' as u16, 0);
        let event = translate_key_event(&record);
        assert_eq!(event.code, KeyCode::Char('a'));
        assert_eq!(event.modifiers, KeyModifiers::empty());
    }

    #[test]
    fn translates_enter_and_escape() {
        assert_eq!(
            translate_key_event(&key_record(0x0D, 0, 0)).code,
            KeyCode::Enter
        );
        assert_eq!(
            translate_key_event(&key_record(0x1B, 0, 0)).code,
            KeyCode::Esc
        );
    }

    #[test]
    fn translates_arrow_keys() {
        assert_eq!(
            translate_key_event(&key_record(0x26, 0, 0)).code,
            KeyCode::Up
        );
        assert_eq!(
            translate_key_event(&key_record(0x27, 0, 0)).code,
            KeyCode::Right
        );
    }

    #[test]
    fn translates_ctrl_combination_modifiers() {
        let event = translate_key_event(&key_record(0x43, 0x03, LEFT_CTRL_PRESSED));
        assert_eq!(event.code, KeyCode::Char('c'));
        assert!(event.modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn translates_shifted_letter_with_shift_modifier() {
        let event = translate_key_event(&key_record(0x41, b'A' as u16, SHIFT_PRESSED));
        assert_eq!(event.code, KeyCode::Char('A'));
        assert!(event.modifiers.contains(KeyModifiers::SHIFT));
    }
}
