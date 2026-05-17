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
        KeyCode::Tab => Action::NextPanel,
        KeyCode::BackTab => Action::PrevPanel,
        KeyCode::Char('j') | KeyCode::Down => Action::ScrollDown,
        KeyCode::Char('k') | KeyCode::Up => Action::ScrollUp,
        KeyCode::Char('l') | KeyCode::Right => Action::NextWindow,
        KeyCode::Char('h') | KeyCode::Left => Action::PrevWindow,
        KeyCode::Char(c) => Panel::from_digit_char(c)
            .map(Action::SwitchPanel)
            .unwrap_or(Action::None),
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    fn char_key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn digits_one_through_panel_count_switch_panels() {
        for digit in 1..=Panel::COUNT {
            let c = char::from_digit(digit as u32, 10).unwrap();
            let expected = Panel::from_index(digit - 1).unwrap();

            assert_eq!(
                handle_key_event(char_key(c), Panel::Overview),
                Action::SwitchPanel(expected)
            );
        }
    }

    #[test]
    fn digits_outside_panel_count_do_not_switch_panels() {
        assert_eq!(
            handle_key_event(char_key('9'), Panel::Overview),
            Action::None
        );
        assert_eq!(
            handle_key_event(char_key('0'), Panel::Overview),
            Action::None
        );
    }
}
