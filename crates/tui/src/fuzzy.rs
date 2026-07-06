//! Built-in interactive fuzzy finder.
//!
//! A tiny, self-contained equivalent of `fzf`: the user types a query and the
//! list of entries is filtered and ranked live; arrow keys move the selection
//! and `Enter` confirms. No external binary is required.
//!
//! Implementation notes (study material):
//!
//! - Raw mode + the alternate screen are used **only** for the picker, and a
//!   [`TerminalGuard`] restores the terminal in its [`Drop`] impl so the user is
//!   never left in a broken terminal — even on panic.
//! - [`fuzzy_score`] is a classic subsequence matcher (the query's characters
//!   must appear, in order, inside the text) with a small score that rewards
//!   consecutive and early matches, very much like `fzf`/Sublime/VS Code.

use std::io::{self, Write};
use std::time::Duration;

use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::{Attribute, Print, SetAttribute};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, size, Clear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::{execute, queue};

/// One selectable row. `label` is what the user sees; `value` is returned.
pub struct Entry {
    pub label: String,
    pub value: String,
}

/// RAII guard that restores the terminal when dropped.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

/// Open the fuzzy picker over `entries`. Returns the chosen `value`, or `None`
/// if the user cancels (`Esc` / `Ctrl-C`). An empty `entries` returns `None`
/// immediately.
pub fn pick(entries: &[Entry], prompt: &str) -> Option<String> {
    if entries.is_empty() {
        return None;
    }

    // Guard first: no matter how we leave this function (return, early return,
    // or panic) the terminal is restored.
    let _guard = TerminalGuard;
    if enable_raw_mode().is_err() {
        // A terminal that cannot enter raw mode (e.g. piped stdin) falls back
        // to a plain numbered menu.
        return pick_numbered(entries, prompt);
    }
    let _ = execute!(io::stdout(), EnterAlternateScreen);

    let mut stdout = io::stdout();
    let mut query = String::new();
    let mut selected: usize = 0;

    loop {
        let ranked = rank(&query, entries);
        if selected >= ranked.len() {
            selected = ranked.len().saturating_sub(1);
        }
        let _ = render(&mut stdout, prompt, &query, &ranked, selected);

        if !event::poll(Duration::from_millis(500)).unwrap_or(false) {
            continue;
        }
        let Ok(Event::Key(key)) = event::read() else {
            continue;
        };

        match handle_key(key, &mut query, &mut selected, &ranked) {
            Decision::Select(value) => return Some(value),
            Decision::Cancel => return None,
            Decision::Continue => {}
        }
    }
}

enum Decision {
    Select(String),
    Cancel,
    Continue,
}

fn handle_key(
    key: KeyEvent,
    query: &mut String,
    selected: &mut usize,
    ranked: &[&Entry],
) -> Decision {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Decision::Cancel;
    }
    match key.code {
        KeyCode::Esc => Decision::Cancel,
        KeyCode::Enter => match ranked.get(*selected) {
            Some(entry) => Decision::Select(entry.value.clone()),
            None => Decision::Continue,
        },
        KeyCode::Backspace => {
            query.pop();
            *selected = 0;
            Decision::Continue
        }
        KeyCode::Char(ch) => {
            query.push(ch);
            *selected = 0;
            Decision::Continue
        }
        KeyCode::Down | KeyCode::Tab => {
            if !ranked.is_empty() {
                *selected = (*selected + 1).min(ranked.len() - 1);
            }
            Decision::Continue
        }
        KeyCode::Up => {
            *selected = selected.saturating_sub(1);
            Decision::Continue
        }
        _ => Decision::Continue,
    }
}

fn render(
    stdout: &mut impl Write,
    prompt: &str,
    query: &str,
    ranked: &[&Entry],
    selected: usize,
) -> io::Result<()> {
    let (_cols, rows) = size().unwrap_or((80, 24));
    let max_rows = (rows as usize).saturating_sub(5);

    queue!(stdout, MoveTo(0, 0), Clear(ClearType::All))?;
    queue!(
        stdout,
        Print(prompt),
        Print("  (type to search · \u{2191}\u{2193} move · Enter select · Esc cancel)\n")
    )?;
    queue!(stdout, Print("search> "), Print(query), Print("\n\n"))?;

    if ranked.is_empty() {
        queue!(stdout, Print("  (no matches)\n"))?;
    }
    for (index, entry) in ranked.iter().take(max_rows).enumerate() {
        let marker = if index == selected { "\u{25b6} " } else { "  " };
        let line = format!("{marker}{}\n", entry.label);
        if index == selected {
            queue!(
                stdout,
                SetAttribute(Attribute::Reverse),
                Print(line),
                SetAttribute(Attribute::NoReverse)
            )?;
        } else {
            queue!(stdout, Print(line))?;
        }
    }
    stdout.flush()?;
    Ok(())
}

/// Rank entries by fuzzy score (desc), then alphabetically.
///
/// An empty query returns the entries in their original order (the adapter
/// already sorts them active-first then alphabetically), so the picker opens
/// with a sensible default list.
fn rank<'a>(query: &str, entries: &'a [Entry]) -> Vec<&'a Entry> {
    if query.trim().is_empty() {
        return entries.iter().collect();
    }
    let mut scored: Vec<(&Entry, i64)> = entries
        .iter()
        .filter_map(|entry| fuzzy_score(query, &entry.label).map(|score| (entry, score)))
        .collect();
    scored.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.label.to_lowercase().cmp(&b.0.label.to_lowercase()))
    });
    scored.into_iter().map(|(entry, _)| entry).collect()
}

/// Subsequence fuzzy match with a small score.
///
/// Returns `None` when `query` is not a subsequence of `text` (case-insensitive).
/// An empty query matches everything with score `0`. Consecutive and earlier
/// matches score higher.
fn fuzzy_score(query: &str, text: &str) -> Option<i64> {
    let query: Vec<char> = query.to_lowercase().chars().collect();
    if query.is_empty() {
        return Some(0);
    }
    let text: Vec<char> = text.to_lowercase().chars().collect();

    let mut qi = 0usize;
    let mut score: i64 = 0;
    let mut consecutive = 0;
    let mut prev_match = false;
    let mut first: i64 = -1;

    for (i, ch) in text.iter().enumerate() {
        if qi < query.len() && *ch == query[qi] {
            if first < 0 {
                first = i as i64;
            }
            consecutive = if prev_match { consecutive + 1 } else { 1 };
            score += 5 + consecutive;
            qi += 1;
            prev_match = true;
        } else {
            prev_match = false;
            consecutive = 0;
        }
    }

    if qi == query.len() {
        Some(score - first) // earlier first match -> higher score
    } else {
        None
    }
}

/// Plain numbered menu used when raw mode is unavailable.
fn pick_numbered(entries: &[Entry], prompt: &str) -> Option<String> {
    println!("\n{prompt}");
    for (index, entry) in entries.iter().enumerate() {
        println!("  {:>2}) {}", index + 1, entry.label);
    }
    print!("\nSelect number (blank to cancel): ");
    let _ = io::stdout().flush();
    let mut buffer = String::new();
    let _ = io::stdin().read_line(&mut buffer);
    let buffer = buffer.trim();
    if buffer.is_empty() {
        return None;
    }
    let index: usize = buffer.parse().ok()?;
    entries
        .get(index.checked_sub(1)?)
        .map(|entry| entry.value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_everything_with_zero_score() {
        assert_eq!(fuzzy_score("", "Anything"), Some(0));
    }

    #[test]
    fn subsequence_match_is_case_insensitive() {
        assert!(fuzzy_score("hom", "Home Network").is_some());
        assert!(fuzzy_score("HOM", "home network").is_some());
    }

    #[test]
    fn non_subsequence_does_not_match() {
        assert!(fuzzy_score("xyz", "Home").is_none());
        assert!(fuzzy_score("hxm", "home").is_none()); // order matters
    }

    #[test]
    fn earlier_match_scores_higher() {
        let early = fuzzy_score("a", "apple").unwrap();
        let late = fuzzy_score("e", "apple").unwrap(); // 'e' at index 4
        assert!(early > late);
    }

    fn entries() -> Vec<Entry> {
        ["Home", "Home-5G", "Guest", "Cafe"]
            .into_iter()
            .map(|label| Entry {
                label: label.to_string(),
                value: label.to_string(),
            })
            .collect()
    }

    #[test]
    fn empty_query_preserves_input_order() {
        let entries = entries();
        let ranked = rank("", &entries);
        let labels: Vec<&str> = ranked.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, vec!["Home", "Home-5G", "Guest", "Cafe"]);
    }

    #[test]
    fn query_filters_and_orders_by_score() {
        let entries = entries();
        let ranked = rank("hom", &entries);
        let labels: Vec<&str> = ranked.iter().map(|e| e.label.as_str()).collect();
        // "Home" and "Home-5G" match; both start with the query so they lead.
        assert!(labels.starts_with(&["Home", "Home-5G"]));
        assert!(!labels.contains(&"Guest"));
        assert!(!labels.contains(&"Cafe"));
    }
}
