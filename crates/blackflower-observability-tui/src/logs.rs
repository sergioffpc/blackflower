use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, TryRecvError};

use blackflower_observability::{ForegroundLogControl, ForegroundLogEvent, ForegroundLogLevel};
use regex::Regex;

const MAX_LOG_EVENTS: usize = 10_000;

pub struct LogState {
    receiver: Receiver<ForegroundLogEvent>,
    pub control: ForegroundLogControl,
    events: VecDeque<BufferedLogEvent>,
    pub view_level: ForegroundLogLevel,
    pub filter_source: String,
    filter: Option<Regex>,
    pub filter_editor: Option<FilterEditor>,
    pub follow: bool,
    pub paused: bool,
    scroll_from_bottom: usize,
    disconnected: bool,
}

pub struct FilterEditor {
    pub draft: String,
    pub error: Option<String>,
}

struct BufferedLogEvent {
    event: ForegroundLogEvent,
    searchable_text: String,
}

impl LogState {
    pub fn new(
        receiver: Receiver<ForegroundLogEvent>,
        control: ForegroundLogControl,
        view_level: ForegroundLogLevel,
        initial_filter: Option<&str>,
    ) -> Result<Self, regex::Error> {
        let filter_source = initial_filter.unwrap_or_default().to_owned();
        let filter = if filter_source.is_empty() {
            None
        } else {
            Some(Regex::new(&filter_source)?)
        };
        Ok(Self {
            receiver,
            control,
            events: VecDeque::with_capacity(MAX_LOG_EVENTS),
            view_level,
            filter_source,
            filter,
            filter_editor: None,
            follow: true,
            paused: false,
            scroll_from_bottom: 0,
            disconnected: false,
        })
    }

    pub fn drain(&mut self) {
        loop {
            match self.receiver.try_recv() {
                Ok(event) => self.push(event),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.disconnected = true;
                    return;
                }
            }
        }
    }

    fn push(&mut self, event: ForegroundLogEvent) {
        let searchable_text = event.searchable_text();
        let visible = self.matches(event.level, &searchable_text);
        if self.events.len() == MAX_LOG_EVENTS {
            let removed_visible = self
                .events
                .front()
                .is_some_and(|front| self.matches_buffered(front));
            self.events.pop_front();
            if removed_visible {
                self.scroll_from_bottom = self.scroll_from_bottom.saturating_sub(1);
            }
        }
        self.events.push_back(BufferedLogEvent {
            event,
            searchable_text,
        });
        if visible && !self.follow {
            self.scroll_from_bottom = self.scroll_from_bottom.saturating_add(1);
        }
    }

    pub fn visible(&self, rows: usize) -> (Vec<&ForegroundLogEvent>, usize, usize) {
        let matches = self
            .events
            .iter()
            .filter(|event| self.matches_buffered(event))
            .map(|event| &event.event)
            .collect::<Vec<_>>();
        let total = matches.len();
        let end = total.saturating_sub(self.scroll_from_bottom.min(total));
        let start = end.saturating_sub(rows);
        (matches[start..end].to_vec(), start, total)
    }

    pub fn recent(&self, rows: usize) -> Vec<&ForegroundLogEvent> {
        let matches = self
            .events
            .iter()
            .filter(|event| self.matches_buffered(event))
            .map(|event| &event.event)
            .collect::<Vec<_>>();
        matches[matches.len().saturating_sub(rows)..].to_vec()
    }

    pub fn cycle_view_level(&mut self) {
        self.view_level = self.view_level.next();
        self.follow();
    }

    pub fn cycle_capture_level(&mut self) {
        self.control.set_level(self.control.level().next());
    }

    pub fn begin_filter_edit(&mut self) {
        self.filter_editor = Some(FilterEditor {
            draft: self.filter_source.clone(),
            error: None,
        });
    }

    pub fn edit_filter_character(&mut self, character: char) {
        if let Some(editor) = &mut self.filter_editor {
            editor.draft.push(character);
            editor.error = None;
        }
    }

    pub fn edit_filter_backspace(&mut self) {
        if let Some(editor) = &mut self.filter_editor {
            editor.draft.pop();
            editor.error = None;
        }
    }

    pub fn commit_filter(&mut self) {
        let Some(editor) = &mut self.filter_editor else {
            return;
        };
        if editor.draft.is_empty() {
            self.filter = None;
            self.filter_source.clear();
            self.filter_editor = None;
            self.follow();
            return;
        }
        match Regex::new(&editor.draft) {
            Ok(regex) => {
                self.filter = Some(regex);
                self.filter_source.clone_from(&editor.draft);
                self.filter_editor = None;
                self.follow();
            }
            Err(error) => editor.error = Some(error.to_string()),
        }
    }

    pub fn cancel_filter_edit(&mut self) {
        self.filter_editor = None;
    }

    pub fn clear_filter(&mut self) {
        self.filter = None;
        self.filter_source.clear();
        self.follow();
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
        self.follow();
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        if self.paused {
            self.follow = false;
        } else {
            self.follow();
        }
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.follow = false;
        self.scroll_from_bottom = self.scroll_from_bottom.saturating_add(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll_from_bottom = self.scroll_from_bottom.saturating_sub(amount);
        if self.scroll_from_bottom == 0 {
            self.follow = true;
            self.paused = false;
        }
    }

    pub fn follow(&mut self) {
        self.follow = true;
        self.paused = false;
        self.scroll_from_bottom = 0;
    }

    pub const fn disconnected(&self) -> bool {
        self.disconnected
    }

    fn matches_buffered(&self, event: &BufferedLogEvent) -> bool {
        self.matches(event.event.level, &event.searchable_text)
    }

    fn matches(&self, level: ForegroundLogLevel, searchable_text: &str) -> bool {
        if !self.view_level.includes(level) {
            return false;
        }
        self.filter
            .as_ref()
            .is_none_or(|regex| regex.is_match(searchable_text))
    }
}

#[cfg(test)]
#[path = "../tests/unit/logs.rs"]
mod tests;
