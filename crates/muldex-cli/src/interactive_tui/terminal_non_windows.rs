use std::io::{self, Write};

use crossterm::cursor::{Hide, SetCursorStyle, Show};
use crossterm::execute;
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

pub(crate) struct TuiTerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    _guard: Option<TerminalModeGuard>,
}

impl TuiTerminalSession {
    pub(crate) fn start(manage_lifecycle: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let guard = if manage_lifecycle {
            Some(TerminalModeGuard::enter()?)
        } else {
            None
        };
        let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
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
        execute!(self.terminal.backend_mut(), Hide)?;
        Ok(())
    }

    pub(crate) fn reset_cursor(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        execute!(
            self.terminal.backend_mut(),
            Show,
            SetCursorStyle::DefaultUserShape
        )?;
        Ok(())
    }

    pub(crate) fn clear_scrollback(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        execute!(
            self.terminal.backend_mut(),
            Clear(ClearType::Purge),
            Clear(ClearType::All),
            crossterm::cursor::MoveTo(0, 0)
        )?;
        Ok(())
    }

    pub(crate) fn invalidate_viewport(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.terminal.clear()?;
        Ok(())
    }

    pub(crate) fn set_cursor_style(
        &mut self,
        style: SetCursorStyle,
    ) -> Result<(), Box<dyn std::error::Error>> {
        execute!(self.terminal.backend_mut(), style)?;
        Ok(())
    }

    pub(crate) fn suspend(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.reset_cursor()?;
        self._guard = None;
        self.terminal.backend_mut().flush()?;
        Ok(())
    }

    pub(crate) fn resume(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self._guard = Some(TerminalModeGuard::enter()?);
        self.terminal.clear()?;
        self.set_cursor_hidden()?;
        Ok(())
    }
}

struct TerminalModeGuard {
    raw_enabled: bool,
    alternate_screen: bool,
}

impl TerminalModeGuard {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut guard = Self {
            raw_enabled: true,
            alternate_screen: false,
        };
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen, Hide) {
            let _ = terminal::disable_raw_mode();
            guard.raw_enabled = false;
            return Err(error);
        }
        guard.alternate_screen = true;
        Ok(guard)
    }
}

impl Drop for TerminalModeGuard {
    fn drop(&mut self) {
        if self.alternate_screen {
            let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
            self.alternate_screen = false;
        }
        if self.raw_enabled {
            let _ = terminal::disable_raw_mode();
            self.raw_enabled = false;
        }
    }
}
