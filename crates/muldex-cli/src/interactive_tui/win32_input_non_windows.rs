use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent};

pub(crate) enum ConsoleInput {
    Key(KeyEvent),
    Resize,
    None,
}

pub(crate) fn poll_console_input() -> io::Result<bool> {
    event::poll(Duration::from_millis(0))
}

pub(crate) fn read_console_input() -> io::Result<ConsoleInput> {
    match event::read()? {
        Event::Key(key_event) => Ok(ConsoleInput::Key(key_event)),
        Event::Resize(_, _) => Ok(ConsoleInput::Resize),
        _ => Ok(ConsoleInput::None),
    }
}
