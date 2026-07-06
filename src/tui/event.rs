use std::{sync::mpsc, thread, time::Duration};

use anyhow::Result;
use crossterm::event::{self, KeyEvent, KeyEventKind, MouseEvent};

pub enum TuiEvent {
    Tick,
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
}

pub struct EventHandler {
    rx: mpsc::Receiver<TuiEvent>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        let event_tx = tx.clone();

        thread::spawn(move || {
            loop {
                if event::poll(tick_rate).unwrap_or(false) {
                    match event::read() {
                        Ok(event::Event::Key(key)) if should_forward_key_event(&key) => {
                            if event_tx.send(TuiEvent::Key(key)).is_err() {
                                break;
                            }
                        }
                        Ok(event::Event::Mouse(mouse)) => {
                            if event_tx.send(TuiEvent::Mouse(mouse)).is_err() {
                                break;
                            }
                        }
                        Ok(event::Event::Resize(width, height)) => {
                            if event_tx.send(TuiEvent::Resize(width, height)).is_err() {
                                break;
                            }
                        }
                        _ => {
                            if event_tx.send(TuiEvent::Tick).is_err() {
                                break;
                            }
                        }
                    }
                } else if event_tx.send(TuiEvent::Tick).is_err() {
                    break;
                }
            }
        });

        drop(tx);
        Self { rx }
    }

    pub fn recv(&mut self) -> Result<TuiEvent> {
        Ok(self.rx.recv()?)
    }
}

fn should_forward_key_event(key: &KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::should_forward_key_event;

    fn key(kind: KeyEventKind) -> KeyEvent {
        KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn forwards_press_and_repeat_but_not_release() {
        assert!(should_forward_key_event(&key(KeyEventKind::Press)));
        assert!(should_forward_key_event(&key(KeyEventKind::Repeat)));
        assert!(!should_forward_key_event(&key(KeyEventKind::Release)));
    }
}
