use std::io;
use std::io::Write;

use crossterm::cursor::SetCursorStyle;
use ratatui::Terminal;
use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Console::{
    CONSOLE_SCREEN_BUFFER_INFO, COORD, CONSOLE_TEXTMODE_BUFFER, CreateConsoleScreenBuffer,
    ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT, ENABLE_VIRTUAL_TERMINAL_INPUT,
    ENABLE_WINDOW_INPUT, GetConsoleMode, GetConsoleScreenBufferInfo, GetStdHandle,
    SetConsoleActiveScreenBuffer, SetConsoleMode, SetConsoleScreenBufferSize, STD_INPUT_HANDLE,
    STD_OUTPUT_HANDLE,
};

use crate::interactive_tui::wincon_backend::WinConBackend;

pub(crate) struct TuiTerminalSession {
    terminal: Terminal<WinConBackend>,
    _guard: Option<TerminalModeGuard<Win32Lifecycle>>,
}

impl TuiTerminalSession {
    pub(crate) fn start(manage_lifecycle: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let guard = if manage_lifecycle {
            Some(TerminalModeGuard::enter(Win32Lifecycle::default())?)
        } else {
            None
        };
        let handle = guard
            .as_ref()
            .map(|g| g.screen_buffer_handle())
            .unwrap_or_else(|| unsafe { GetStdHandle(STD_OUTPUT_HANDLE) });
        let backend = WinConBackend::with_handle(io::stdout(), handle)?;
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;
        Ok(Self {
            terminal,
            _guard: guard,
        })
    }

    pub(crate) fn draw<F>(&mut self, render_fn: F) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnOnce(&mut ratatui::Frame<'_>),
    {
        self.terminal.draw(render_fn)?;
        Ok(())
    }

    pub(crate) fn set_cursor_bar(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.set_cursor_style(SetCursorStyle::SteadyBar)
    }

    pub(crate) fn set_cursor_hidden(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.terminal.backend().hide_cursor_now()?;
        Ok(())
    }

    pub(crate) fn reset_cursor(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.set_cursor_style(SetCursorStyle::DefaultUserShape)
    }

    pub(crate) fn clear_scrollback(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.terminal.backend().clear_scrollback()?;
        Ok(())
    }

    /// Force a full redraw on the next frame by clearing ratatui's backing
    /// buffer (GAP-01 viewport invalidation).
    pub(crate) fn invalidate_viewport(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.terminal.clear()?;
        Ok(())
    }

    pub(crate) fn set_cursor_style(
        &mut self,
        style: SetCursorStyle,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.terminal.backend().set_cursor_style(style)?;
        Ok(())
    }

    /// Temporarily restore the terminal (raw mode off, leave alternate
    /// screen) so an external program (e.g. a text editor) can take over
    /// stdout. Caller must call `resume` afterwards.
    pub(crate) fn suspend(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.reset_cursor()?;
        // dropping the guard restores raw mode and leaves the alternate screen
        self._guard = None;
        io::stdout().flush()?;
        Ok(())
    }

    /// Re-enter the TUI terminal after `suspend`.
    pub(crate) fn resume(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self._guard = Some(TerminalModeGuard::enter(Win32Lifecycle::default())?);
        let handle = self
            ._guard
            .as_ref()
            .map(|g| g.screen_buffer_handle())
            .unwrap_or_else(|| unsafe { GetStdHandle(STD_OUTPUT_HANDLE) });
        self.terminal.backend().set_handle(handle);
        self.terminal.clear()?;
        self.set_cursor_hidden()?;
        Ok(())
    }
}

pub(crate) trait TerminalLifecycle {
    fn enable_raw_mode(&mut self) -> io::Result<()>;
    fn disable_raw_mode(&mut self) -> io::Result<()>;
    fn enter_alternate_screen(&mut self) -> io::Result<()>;
    fn leave_alternate_screen(&mut self) -> io::Result<()>;
}

/// Provides the console screen-buffer handle the backend should paint to once
/// the lifecycle has entered the alternate screen.
pub(crate) trait AlternateScreenHandle {
    fn screen_buffer_handle(&self) -> HANDLE;
}

#[derive(Default)]
pub(crate) struct Win32Lifecycle {
    input_handle: HANDLE,
    saved_input_mode: u32,
    default_screen: HANDLE,
    alternate_screen: Option<HANDLE>,
}

impl TerminalLifecycle for Win32Lifecycle {
    fn enable_raw_mode(&mut self) -> io::Result<()> {
        let input = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        if input == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let mut mode = 0u32;
        if unsafe { GetConsoleMode(input, &mut mode) } == 0 {
            return Err(io::Error::last_os_error());
        }
        self.input_handle = input;
        self.saved_input_mode = mode;
        // Disable cooked input, enable window/VT input for raw TUI behavior.
        let new_mode = mode
            & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT)
            | ENABLE_WINDOW_INPUT
            | ENABLE_VIRTUAL_TERMINAL_INPUT;
        if unsafe { SetConsoleMode(input, new_mode) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        if self.input_handle != 0 && self.input_handle != INVALID_HANDLE_VALUE {
            unsafe {
                let _ = SetConsoleMode(self.input_handle, self.saved_input_mode);
            }
        }
        Ok(())
    }

    fn enter_alternate_screen(&mut self) -> io::Result<()> {
        let default_screen = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        self.default_screen = default_screen;
        let handle = unsafe {
            CreateConsoleScreenBuffer(
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                CONSOLE_TEXTMODE_BUFFER,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // Match the new buffer's size to the visible console window so VT
        // painting (delegated to the inner backend) maps correctly.
        let mut info = unsafe { std::mem::zeroed::<CONSOLE_SCREEN_BUFFER_INFO>() };
        if unsafe { GetConsoleScreenBufferInfo(default_screen, &mut info) } != 0 {
            unsafe {
                let _ = SetConsoleScreenBufferSize(handle, info.dwSize);
            }
        }
        if unsafe { SetConsoleActiveScreenBuffer(handle) } == 0 {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(io::Error::last_os_error());
        }
        self.alternate_screen = Some(handle);
        Ok(())
    }

    fn leave_alternate_screen(&mut self) -> io::Result<()> {
        if let Some(handle) = self.alternate_screen.take() {
            unsafe {
                let _ = SetConsoleActiveScreenBuffer(self.default_screen);
                let _ = CloseHandle(handle);
            }
        }
        Ok(())
    }
}

impl AlternateScreenHandle for Win32Lifecycle {
    fn screen_buffer_handle(&self) -> HANDLE {
        self.alternate_screen.unwrap_or(self.default_screen)
    }
}

#[derive(Debug)]
struct TerminalModeGuard<L: TerminalLifecycle + AlternateScreenHandle> {
    lifecycle: L,
    raw_enabled: bool,
    alternate_screen: bool,
}

impl<L: TerminalLifecycle + AlternateScreenHandle> TerminalModeGuard<L> {
    fn enter(mut lifecycle: L) -> io::Result<Self> {
        lifecycle.enable_raw_mode()?;
        let mut guard = Self {
            lifecycle,
            raw_enabled: true,
            alternate_screen: false,
        };

        if let Err(error) = guard.lifecycle.enter_alternate_screen() {
            let _ = guard.lifecycle.disable_raw_mode();
            guard.raw_enabled = false;
            return Err(error);
        }

        guard.alternate_screen = true;
        Ok(guard)
    }

    fn screen_buffer_handle(&self) -> HANDLE {
        self.lifecycle.screen_buffer_handle()
    }
}

impl<L: TerminalLifecycle + AlternateScreenHandle> Drop for TerminalModeGuard<L> {
    fn drop(&mut self) {
        if self.alternate_screen {
            let _ = self.lifecycle.leave_alternate_screen();
            self.alternate_screen = false;
        }
        if self.raw_enabled {
            let _ = self.lifecycle.disable_raw_mode();
            self.raw_enabled = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug)]
    struct FakeLifecycle {
        events: Arc<Mutex<Vec<&'static str>>>,
        fail_enter_alternate: bool,
    }

    impl FakeLifecycle {
        fn new(fail_enter_alternate: bool) -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
                fail_enter_alternate,
            }
        }

        fn events(&self) -> Vec<&'static str> {
            self.events.lock().expect("events").clone()
        }
    }

    impl TerminalLifecycle for FakeLifecycle {
        fn enable_raw_mode(&mut self) -> io::Result<()> {
            self.events.lock().expect("events").push("enable_raw");
            Ok(())
        }

        fn disable_raw_mode(&mut self) -> io::Result<()> {
            self.events.lock().expect("events").push("disable_raw");
            Ok(())
        }

        fn enter_alternate_screen(&mut self) -> io::Result<()> {
            self.events.lock().expect("events").push("enter_alt");
            if self.fail_enter_alternate {
                Err(io::Error::other("enter alt failed"))
            } else {
                Ok(())
            }
        }

        fn leave_alternate_screen(&mut self) -> io::Result<()> {
            self.events.lock().expect("events").push("leave_alt");
            Ok(())
        }
    }

    impl AlternateScreenHandle for FakeLifecycle {
        fn screen_buffer_handle(&self) -> HANDLE {
            0
        }
    }

    #[test]
    fn guard_creation_is_abstracted_behind_testable_lifecycle() {
        let lifecycle = FakeLifecycle::new(false);
        let mirror = lifecycle.clone();
        let guard = TerminalModeGuard::enter(lifecycle).expect("enter guard");
        drop(guard);
        assert_eq!(
            mirror.events(),
            vec!["enable_raw", "enter_alt", "leave_alt", "disable_raw"]
        );
    }

    #[test]
    fn failure_paths_restore_state_through_guard_drop_semantics() {
        let lifecycle = FakeLifecycle::new(true);
        let mirror = lifecycle.clone();
        let error = TerminalModeGuard::enter(lifecycle).expect_err("guard should fail");
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(
            mirror.events(),
            vec!["enable_raw", "enter_alt", "disable_raw"]
        );
    }
}
