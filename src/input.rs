use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::{Action, App, Effect};

impl App {
    pub fn key(&mut self, key: KeyEvent, now: u64) -> Vec<Effect> {
        if key.kind == KeyEventKind::Release {
            return Vec::new();
        }
        let action = if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => Action::Quit,
                KeyCode::Char('z') => Action::Suspend,
                _ => return Vec::new(),
            }
        } else if self.too_small {
            match key.code {
                KeyCode::Char('q') => Action::Quit,
                KeyCode::Char(' ') => Action::TogglePlayback,
                KeyCode::Char('+') | KeyCode::Char('=') => Action::Volume(5),
                KeyCode::Char('-') => Action::Volume(-5),
                _ => return Vec::new(),
            }
        } else if self.help_open {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') => Action::CloseOverlay,
                KeyCode::Char('q') => Action::Quit,
                _ => return Vec::new(),
            }
        } else if self.form_open {
            match key.code {
                KeyCode::Char(ch) if !ch.is_control() => Action::Input(ch),
                KeyCode::Backspace => Action::Backspace,
                KeyCode::Tab => Action::NextField,
                KeyCode::Enter => Action::SubmitSearch,
                KeyCode::Esc => Action::CloseOverlay,
                _ => return Vec::new(),
            }
        } else {
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => Action::Move(1),
                KeyCode::Up | KeyCode::Char('k') => Action::Move(-1),
                KeyCode::Enter => Action::Play,
                KeyCode::Char(' ') => Action::TogglePlayback,
                KeyCode::Char('f') => Action::ToggleFavorite,
                KeyCode::Char('+') | KeyCode::Char('=') => Action::Volume(5),
                KeyCode::Char('-') => Action::Volume(-5),
                KeyCode::Tab => Action::SwitchView,
                KeyCode::Char('/') => Action::OpenSearch,
                KeyCode::PageDown => Action::Page(1),
                KeyCode::PageUp => Action::Page(-1),
                KeyCode::Char('?') => Action::Help,
                KeyCode::Char('q') => Action::Quit,
                _ => return Vec::new(),
            }
        };
        self.update(action, now)
    }
}
