pub(crate) mod frames;
pub(crate) mod keymap;
pub(crate) mod layout;
pub(crate) mod markdown_render;
pub(crate) mod notifications;
pub(crate) mod overlay;
pub(crate) mod render;
pub(crate) mod terminal;
pub(crate) mod terminal_palette;
pub(crate) mod theme;
pub(crate) mod view_model;
pub(crate) mod win32_input;
pub(crate) mod wincon_backend;

pub(crate) use keymap::RuntimeKeymap;
pub(crate) use terminal::TuiTerminalSession;
pub(crate) use view_model::ShellViewModel;

pub(crate) fn draw(
    session: &mut TuiTerminalSession,
    view_model: &ShellViewModel,
) -> Result<(), Box<dyn std::error::Error>> {
    session.draw(|frame| render::render_shell(frame, view_model))?;
    Ok(())
}

pub(crate) fn set_cursor_bar(
    session: &mut TuiTerminalSession,
) -> Result<(), Box<dyn std::error::Error>> {
    session.set_cursor_bar()
}

pub(crate) fn start_terminal_session(
    manage_lifecycle: bool,
) -> Result<TuiTerminalSession, Box<dyn std::error::Error>> {
    TuiTerminalSession::start(manage_lifecycle)
}

#[cfg(test)]
pub(crate) fn render_to_buffer(
    view_model: &ShellViewModel,
    width: u16,
    height: u16,
) -> Result<ratatui::buffer::Buffer, Box<dyn std::error::Error>> {
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend)?;
    terminal.draw(|frame| render::render_shell(frame, view_model))?;
    Ok(terminal.backend().buffer().clone())
}
