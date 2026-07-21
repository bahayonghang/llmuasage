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
    PageDown,
    PageUp,
    SelectFirst,
    SelectLast,
    CycleSort,
    ReverseSort,
    NextWindow,
    PrevWindow,
    Refresh,
    ToggleAutoRefresh,
    StartSync,
    OpenSourcePicker,
    OpenHelp,
    CycleTheme,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogAction {
    Close,
    MoveDown,
    MoveUp,
    Select,
    ClearSource,
    None,
}

pub fn handle_key_event(key: KeyEvent, _active_panel: Panel) -> Action {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Tab => Action::NextPanel,
        KeyCode::BackTab => Action::PrevPanel,
        KeyCode::Char('j') | KeyCode::Down => Action::ScrollDown,
        KeyCode::Char('k') | KeyCode::Up => Action::ScrollUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::Home => Action::SelectFirst,
        KeyCode::End => Action::SelectLast,
        KeyCode::Char('o') => Action::CycleSort,
        KeyCode::Char('O') => Action::ReverseSort,
        KeyCode::Char('l') | KeyCode::Right => Action::NextWindow,
        KeyCode::Char('h') | KeyCode::Left => Action::PrevWindow,
        KeyCode::Char('r') => Action::Refresh,
        KeyCode::Char('R') => Action::ToggleAutoRefresh,
        KeyCode::Char('x') => Action::StartSync,
        KeyCode::Char('s') => Action::OpenSourcePicker,
        KeyCode::Char('?') => Action::OpenHelp,
        KeyCode::Char('t') => Action::CycleTheme,
        KeyCode::Char(c) => Panel::from_digit_char(c)
            .map(Action::SwitchPanel)
            .unwrap_or(Action::None),
        _ => Action::None,
    }
}

pub fn handle_dialog_key_event(key: KeyEvent) -> DialogAction {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => DialogAction::Close,
        KeyCode::Char('j') | KeyCode::Down => DialogAction::MoveDown,
        KeyCode::Char('k') | KeyCode::Up => DialogAction::MoveUp,
        KeyCode::Enter | KeyCode::Char(' ') => DialogAction::Select,
        KeyCode::Char('a') => DialogAction::ClearSource,
        _ => DialogAction::None,
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
        // With COUNT == 9, only '0' is outside the 1..=9 panel range.
        assert_eq!(
            handle_key_event(char_key('0'), Panel::Overview),
            Action::None
        );
    }

    #[test]
    fn source_picker_shortcut_opens_dialog() {
        assert_eq!(
            handle_key_event(char_key('s'), Panel::Overview),
            Action::OpenSourcePicker
        );
        assert_eq!(
            handle_key_event(char_key('x'), Panel::Overview),
            Action::StartSync
        );
        assert_eq!(
            handle_key_event(char_key('?'), Panel::Overview),
            Action::OpenHelp
        );
    }

    #[test]
    fn paging_and_sort_keys_map_to_actions() {
        assert_eq!(
            handle_key_event(
                KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
                Panel::Models
            ),
            Action::PageDown
        );
        assert_eq!(
            handle_key_event(
                KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
                Panel::Models
            ),
            Action::PageUp
        );
        assert_eq!(
            handle_key_event(
                KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
                Panel::Models
            ),
            Action::SelectFirst
        );
        assert_eq!(
            handle_key_event(
                KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
                Panel::Models
            ),
            Action::SelectLast
        );
        assert_eq!(
            handle_key_event(char_key('o'), Panel::Models),
            Action::CycleSort
        );
        assert_eq!(
            handle_key_event(char_key('O'), Panel::Models),
            Action::ReverseSort
        );
    }

    #[test]
    fn dialog_keys_map_to_source_picker_actions() {
        assert_eq!(
            handle_dialog_key_event(char_key('j')),
            DialogAction::MoveDown
        );
        assert_eq!(
            handle_dialog_key_event(char_key('a')),
            DialogAction::ClearSource
        );
    }
}
