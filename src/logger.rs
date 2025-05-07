use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};
use ratatui::{
    layout::Rect,
    prelude::*,
    text::Line,
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
};

pub struct Logger {
    pub client: std::sync::mpsc::Sender<(Level, String)>,
}

impl Logger {
    pub fn new(sender: std::sync::mpsc::Sender<(Level, String)>) -> Self {
        Self { client: sender }
    }
    pub fn init(self) -> Result<(), SetLoggerError> {
        log::set_max_level(LevelFilter::Trace);
        log::set_boxed_logger(Box::new(self))
    }
}

impl Log for Logger {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        if !record.target().starts_with("btclient") {
            return;
        }

        self.client
            .send((
                record.level(),
                format!(
                    ": {}:{}: {}\n",
                    record
                        .file()
                        .map(|f| f.split('/').next_back().unwrap_or(""))
                        .unwrap_or(""),
                    record.line().unwrap_or(0),
                    record.args()
                ),
            ))
            .unwrap()
    }

    fn flush(&self) {}
}

pub struct LogTab {
    text: Vec<(Level, String)>,
    offset: usize,
    spans: [Span<'static>; 5],
}

impl LogTab {
    pub fn new() -> Self {
        Self {
            text: Vec::new(),
            offset: 0,
            spans: [
                Span::styled("ERROR", Color::LightRed),
                Span::styled("WARN", Color::Yellow),
                Span::styled("INFO", Color::Green),
                Span::styled("DEBUG", Color::White),
                Span::styled("TRACE", Color::White),
            ],
        }
    }

    pub fn push(&mut self, msg: (Level, String)) {
        self.text.push(msg);
    }

    pub fn scroll_up(&mut self) {
        self.offset = std::cmp::min(self.offset + 1, self.text.len());
    }

    pub fn scroll_down(&mut self) {
        self.offset = self.offset.saturating_sub(1);
    }

    pub fn cap_scroll(&mut self, view_height: usize) {
        self.offset = std::cmp::min(self.offset + 1, self.text.len().saturating_sub(view_height));
    }

    //pub fn dump(path: &str) {}
}

impl Widget for &LogTab {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut next_line = Rect { height: 1, ..area };
        let off = std::cmp::min(
            self.offset,
            self.text.len().saturating_sub(area.height as usize),
        );
        let scroll_length = self.text.len().saturating_sub(area.height as usize);

        // does not consider multi-line log messages
        let visible_lines = if self.text.len() > area.height as usize {
            &self.text[self.text.len() - area.height as usize - off..self.text.len() - off]
        } else {
            &self.text
        };

        for l in visible_lines {
            // clones are fine here because Cow has lazy data clones
            let mut line = Line::from(self.spans[l.0 as usize - 1].clone());
            line.push_span(l.1.clone());
            line.render(next_line, buf);
            next_line.y += 1;
        }

        // add scrollbar
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        let mut scrollbar_state = ScrollbarState::new(scroll_length).position(off);
        scrollbar.render(
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            buf,
            &mut scrollbar_state,
        );
    }
}
