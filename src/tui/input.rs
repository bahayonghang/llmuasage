use crossterm::event::{KeyCode, KeyEvent};

use super::app::Panel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    SwitchPanel(Panel),
    NextPanel,
    PrevPanel,
    ScrollDown,
    ScrollUp,
    NextWindow,
    PrevWindow,
    None,
}

pub fn handle_key_event(key: KeyEvent, _active_panel: Panel) -> Action {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Char(c @ '1'..='7') => {
            Action::SwitchPanel(Panel::from_index((c as u8 - b'1') as usize).unwrap())
        }
        KeyCode::Tab => Action::NextPanel,
        KeyCode::BackTab => Action::PrevPanel,
        KeyCode::Char('j') | KeyCode::Down => Action::ScrollDown,
        KeyCode::Char('k') | KeyCode::Up => Action::ScrollUp,
        KeyCode::Char('l') | KeyCode::Right => Action::NextWindow,
        KeyCode::Char('h') | KeyCode::Left => Action::PrevWindow,
        _ => Action::None,
    }
}
