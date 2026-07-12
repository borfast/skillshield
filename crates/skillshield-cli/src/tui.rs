//! Minimal Ratatui screens. Currently a multi-select checkbox picker used by
//! `init` to choose which catalog groups to monitor. The event loop is a thin
//! I/O shell (not unit-tested); the selection logic (`selected_keys`) is pure.

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

/// One selectable row in the picker.
pub struct GroupChoice {
    pub key: String,
    pub label: String,
    pub detail: String,
    pub checked: bool,
}

/// The keys of all currently-checked choices, in order.
pub fn selected_keys(choices: &[GroupChoice]) -> Vec<String> {
    choices
        .iter()
        .filter(|c| c.checked)
        .map(|c| c.key.clone())
        .collect()
}

/// Restores the terminal (raw mode + alternate screen) on drop, so a panic or
/// early return can't leave the user's terminal wedged.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self, String> {
        enable_raw_mode().map_err(|e| e.to_string())?;
        // If entering the alternate screen fails we must undo raw mode here:
        // the guard isn't constructed yet, so its Drop wouldn't run.
        if let Err(e) = execute!(std::io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(e.to_string());
        }
        Ok(TerminalGuard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    }
}

/// Run the checkbox picker. Returns `Some(selected keys)` on confirm (Enter),
/// or `None` if the user cancels (Esc/q). Requires a TTY — callers gate on
/// `IsTerminal` and fall back to defaults otherwise.
pub fn select_groups(
    title: &str,
    mut choices: Vec<GroupChoice>,
) -> Result<Option<Vec<String>>, String> {
    if choices.is_empty() {
        return Ok(Some(vec![]));
    }

    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;

    let mut cursor = 0usize;
    let outcome = loop {
        terminal
            .draw(|f| draw(f, title, &choices, cursor))
            .map_err(|e| e.to_string())?;

        if let Event::Key(key) = event::read().map_err(|e| e.to_string())? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    cursor = cursor.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if cursor + 1 < choices.len() {
                        cursor += 1;
                    }
                }
                KeyCode::Char(' ') => {
                    choices[cursor].checked = !choices[cursor].checked;
                }
                KeyCode::Char('a') => {
                    let target = !choices.iter().all(|c| c.checked);
                    for c in &mut choices {
                        c.checked = target;
                    }
                }
                KeyCode::Enter => break Some(selected_keys(&choices)),
                KeyCode::Esc | KeyCode::Char('q') => break None,
                _ => {}
            }
        }
    };
    Ok(outcome)
}

fn draw(f: &mut Frame, title: &str, choices: &[GroupChoice], cursor: usize) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(f.area());

    let header = Paragraph::new(title)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("SkillShield"));
    f.render_widget(header, chunks[0]);

    let items: Vec<ListItem> = choices
        .iter()
        .map(|c| {
            let mark = if c.checked { "[x]" } else { "[ ]" };
            ListItem::new(format!("{mark}  {}  —  {}", c.label, c.detail))
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Monitor"))
        .highlight_symbol("➤ ")
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut state = ListState::default();
    state.select(Some(cursor));
    f.render_stateful_widget(list, chunks[1], &mut state);

    let footer = Paragraph::new("↑/↓ move   space toggle   a all   Enter confirm   Esc cancel")
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(key: &str, checked: bool) -> GroupChoice {
        GroupChoice {
            key: key.into(),
            label: key.into(),
            detail: String::new(),
            checked,
        }
    }

    #[test]
    fn selected_keys_returns_only_checked_in_order() {
        let choices = vec![choice("a", true), choice("b", false), choice("c", true)];
        assert_eq!(
            selected_keys(&choices),
            vec!["a".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn selected_keys_empty_when_none_checked() {
        let choices = vec![choice("a", false), choice("b", false)];
        assert!(selected_keys(&choices).is_empty());
    }
}
