use std::cell::Cell;
use std::io::{self, Write};

use ratatui::backend::{Backend, WindowSize};
use ratatui::buffer::Cell as BufferCell;
use ratatui::layout::{Position, Size};

use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Console::{
    CONSOLE_CURSOR_INFO, CONSOLE_FONT_INFOEX, CONSOLE_SCREEN_BUFFER_INFO, COORD,
    GetConsoleCursorInfo, GetConsoleScreenBufferInfo, GetCurrentConsoleFontEx, GetStdHandle,
    SetConsoleCursorInfo, SetConsoleCursorPosition, STD_OUTPUT_HANDLE,
};

use crossterm::cursor::SetCursorStyle;
use ratatui::backend::CrosstermBackend;

/// Custom ratatui backend for Windows.
///
/// Cell painting (truecolor/RGB) is delegated to ratatui's `CrosstermBackend`,
/// which emits VT escape sequences so syntax highlighting and adaptive colors
/// keep working. Cursor position/visibility, cursor style, screen size and
/// viewport queries are handled through the native Win32 console API for
/// fine-grained control (GAP-01 / GAP-15).
///
/// The screen-buffer `handle` is stored in a `Cell` so the active screen buffer
/// can be swapped (e.g. when the lifecycle re-enters the alternate screen after
/// an external editor `suspend`/`resume`) without replacing the whole backend.
pub(crate) struct WinConBackend {
    inner: CrosstermBackend<io::Stdout>,
    handle: Cell<HANDLE>,
}

impl WinConBackend {
    pub(crate) fn new(stdout: io::Stdout) -> io::Result<Self> {
        let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        Self::with_handle(stdout, handle)
    }

    pub(crate) fn with_handle(stdout: io::Stdout, handle: HANDLE) -> io::Result<Self> {
        if handle == 0 || handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to obtain stdout console handle",
            ));
        }
        Ok(Self {
            inner: CrosstermBackend::new(stdout),
            handle: Cell::new(handle),
        })
    }

    /// Point the backend at a different screen buffer (used when the lifecycle
    /// swaps the active console screen buffer).
    pub(crate) fn set_handle(&self, handle: HANDLE) {
        self.handle.set(handle);
    }

    /// Set the console cursor shape (block / bar / underline) via the native
    /// Win32 cursor info. The legacy Windows console cannot render a true bar
    /// or underline, so non-block shapes are approximated with a smaller block.
    pub(crate) fn set_cursor_style(&self, style: SetCursorStyle) -> io::Result<()> {
        let size = match style {
            SetCursorStyle::SteadyBlock
            | SetCursorStyle::BlinkingBlock
            | SetCursorStyle::DefaultUserShape => 100,
            _ => 20,
        };
        let mut info = unsafe { std::mem::zeroed::<CONSOLE_CURSOR_INFO>() };
        if unsafe { GetConsoleCursorInfo(        self.handle.get(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        info.dwSize = size;
        info.bVisible = 1;
        if unsafe { SetConsoleCursorInfo(        self.handle.get(), &info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Hide the cursor by toggling its visibility through the Win32 API.
    pub(crate) fn hide_cursor_now(&self) -> io::Result<()> {
        set_cursor_visible(        self.handle.get(), false)
    }

    /// Show the cursor by toggling its visibility through the Win32 API.
    pub(crate) fn show_cursor_now(&self) -> io::Result<()> {
        set_cursor_visible(        self.handle.get(), true)
    }

    /// Clear the scrollback buffer history (ESC[3J) then clear the visible
    /// screen and home the cursor. Kept as a VT sequence because the active
    /// console already has virtual-terminal processing enabled by the
    /// lifecycle layer, and it is the portable equivalent of a full
    /// `ScrollConsoleScreenBufferW` purge.
    pub(crate) fn clear_scrollback(&self) -> io::Result<()> {
        let mut out = io::stdout();
        out.write_all(b"\x1b[3J\x1b[2J\x1b[H")?;
        out.flush()?;
        Ok(())
    }
}

impl Backend for WinConBackend {
    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a BufferCell)>,
    {
        self.inner.draw(content)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        set_cursor_visible(        self.handle.get(), false)
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        set_cursor_visible(        self.handle.get(), true)
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        let info = screen_buffer_info(        self.handle.get())?;
        Ok(Position {
            x: info.dwCursorPosition.X.max(0) as u16,
            y: info.dwCursorPosition.Y.max(0) as u16,
        })
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        let p = position.into();
        write_cursor_position(        self.handle.get(), p.x, p.y)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }

    fn size(&self) -> io::Result<Size> {
        let info = screen_buffer_info(        self.handle.get())?;
        let width = (info.srWindow.Right - info.srWindow.Left + 1).max(0) as u16;
        let height = (info.srWindow.Bottom - info.srWindow.Top + 1).max(0) as u16;
        Ok(Size { width, height })
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        let size = self.size()?;
        let (cell_w, cell_h) = cell_size_pixels(        self.handle.get()).unwrap_or((8, 16));
        Ok(WindowSize {
            columns_rows: Size::new(size.width, size.height),
            pixels: Size::new(
                size.width.saturating_mul(cell_w as u16),
                size.height.saturating_mul(cell_h as u16),
            ),
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.inner)
    }
}

fn screen_buffer_info(handle: HANDLE) -> io::Result<CONSOLE_SCREEN_BUFFER_INFO> {
    let mut info = unsafe { std::mem::zeroed::<CONSOLE_SCREEN_BUFFER_INFO>() };
    if unsafe { GetConsoleScreenBufferInfo(handle, &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info)
}

fn set_cursor_visible(handle: HANDLE, visible: bool) -> io::Result<()> {
    let mut info = unsafe { std::mem::zeroed::<CONSOLE_CURSOR_INFO>() };
    if unsafe { GetConsoleCursorInfo(handle, &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    info.bVisible = if visible { 1 } else { 0 };
    if unsafe { SetConsoleCursorInfo(handle, &info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn write_cursor_position(handle: HANDLE, x: u16, y: u16) -> io::Result<()> {
    let coord = COORD {
        X: x as i16,
        Y: y as i16,
    };
    if unsafe { SetConsoleCursorPosition(handle, coord) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn cell_size_pixels(handle: HANDLE) -> Option<(u16, u16)> {
    let mut info = unsafe { std::mem::zeroed::<CONSOLE_FONT_INFOEX>() };
    info.cbSize = std::mem::size_of::<CONSOLE_FONT_INFOEX>() as u32;
    if unsafe { GetCurrentConsoleFontEx(handle, 1, &mut info) } == 0 {
        return None;
    }
    Some((info.dwFontSize.X.max(0) as u16, info.dwFontSize.Y.max(0) as u16))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_constructs_from_stdout_handle() {
        // Construction only validates the console handle; it must succeed even
        // when stdout is redirected (a pipe still yields a valid handle).
        let backend = WinConBackend::new(io::stdout());
        assert!(backend.is_ok(), "WinConBackend should construct from stdout");
    }

    #[test]
    fn clear_scrollback_emits_escape_sequence_without_panic() {
        if let Ok(backend) = WinConBackend::new(io::stdout()) {
            // Writing the scrollback-clear VT bytes is harmless on a pipe and
            // exercises the method path without requiring a real console.
            let _ = backend.clear_scrollback();
        }
    }
}

