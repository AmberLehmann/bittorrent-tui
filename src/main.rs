use crate::logger::Logger;
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use log::{error, info};
use metainfo::{Info, MetaInfo, SingleFileInfo};
use ratatui::Frame;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::*,
    style::Stylize,
    symbols::border,
    text::Line,
    widgets::{Block, Borders, Paragraph, Widget},
};
use std::fs::File;
use std::io::{Read, Stdout};

mod args;
mod http_messages;
mod logger;
mod metainfo;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

fn main() -> Result<()> {
    let mut terminal = ratatui::init();

    let (tx, rx) = std::sync::mpsc::channel();
    Logger::new(tx).init().unwrap();
    let mut app = App::new(rx);
    let result = app.run(&mut terminal);

    ratatui::restore();
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppTab {
    Downloads,
    Peers,
    Log,
}

struct App {
    exit: bool,
    logtext: Vec<String>,
    rx: std::sync::mpsc::Receiver<String>,
    selected_tab: AppTab,
    torrents: Vec<MetaInfo>,
}

impl App {
    pub fn new(rx: std::sync::mpsc::Receiver<String>) -> Self {
        Self {
            exit: false,
            logtext: Vec::new(),
            rx,
            selected_tab: AppTab::Downloads,
            torrents: Vec::new(),
        }
    }

    /// runs the application's main loop until the user quits
    pub fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.render_frame(frame))?;
            self.handle_events()?;

            // handle logging
            if let Ok(s) = self.rx.try_recv() {
                self.logtext.push(s);
            }

            // handle bittorrent stuff
        }
        Ok(())
    }

    fn render_frame(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    /// updates the application's state based on user input
    fn handle_events(&mut self) -> Result<()> {
        if event::poll(std::time::Duration::from_millis(16))? {
            match event::read()? {
                // it's important to check that the event is a key press event as
                // crossterm also emits key release and repeat events on Windows.
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    self.handle_key_event(key_event)?
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Char('q') => self.exit = true,
            KeyCode::Char('d') | KeyCode::Char('1') => self.selected_tab = AppTab::Downloads,
            KeyCode::Char('p') | KeyCode::Char('2') => self.selected_tab = AppTab::Peers,
            KeyCode::Char('l') | KeyCode::Char('3') => self.selected_tab = AppTab::Log,
            KeyCode::Char('o') => self.open_torrent("./alice.torrent"),
            _ => {}
        }
        Ok(())
    }

    fn open_torrent(&mut self, path: &str) {
        let mut file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                error!("failed to open torrent file {:?}", e);
                return;
            }
        };

        let mut data = Vec::new();
        let bytes_read = file.read_to_end(&mut data);
        info!("open_trrent read {:?} bytes", bytes_read);

        let new_torrent = match serde_bencode::from_bytes(&data) {
            Ok(t) => t,
            Err(e) => {
                error!("{}", e);
                return;
            }
        };

        self.torrents.push(new_torrent);
    }

    // RENDERING CODE

    fn render_downloads(&self, area: Rect, buf: &mut Buffer) {
        let vertical = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]);
        let [title_bar, mut canvas] = vertical.areas(area);
        let horizontal = Layout::horizontal([
            Constraint::Length(2),  // icon
            Constraint::Min(20),    // name
            Constraint::Length(12), // size
            Constraint::Length(12), // progress
            Constraint::Length(13), // status
            Constraint::Length(10), // seeds
            Constraint::Length(10), // peers
            Constraint::Length(14), // speed
            Constraint::Length(6),  // todo
        ]);

        let columns = [
            "", "Name", "Size", "Progress", "Status", "Seeds", "Peers", "Speed", "TODO",
        ];
        let title_bar_areas: [_; 9] = horizontal.areas(title_bar);
        Block::new()
            .style(Style::new().bg(Color::Black))
            .render(title_bar, buf);
        for (i, &t) in columns.iter().enumerate() {
            Span::raw(t).render(title_bar_areas[i], buf);
        }

        for torrent in &self.torrents {
            let [_icon, name, size, _progress, _status, _seeds, _peers, _speed, _todo] =
                horizontal.areas(canvas);

            match &torrent.info {
                Info::Multi(_) => {}
                Info::Single(f) => {
                    Span::raw(&f.name).render(name, buf);
                    Span::raw(format!("{}", f.length)).render(size, buf);
                }
            }

            canvas.y += 1;
        }

        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("🡅 ", Style::new().fg(Color::LightGreen)),
                Span::styled(
                    "minecraft-movie-but-real-this-time      1.4 GiB  ",
                    Style::new(),
                ),
                Span::styled(
                    "    50",
                    Style::new().fg(Color::Black).bg(Color::LightGreen),
                ),
                Span::styled("%     ", Style::new().fg(Color::Black).bg(Color::Gray)),
                Span::styled("  Seeding  0/250    4/7    7.9 MiB/s   999.7", Style::new()),
            ]),
            Line::from(vec![
                Span::styled("🡅 ", Style::new().fg(Color::LightGreen)),
                Span::styled(
                    "totally-not-a-digital-cat              35.8 GiB  ",
                    Style::new(),
                ),
                Span::styled(
                    "   100%     ",
                    Style::new().fg(Color::Black).bg(Color::LightGreen),
                ),
                Span::styled("  Seeding  78/170  14/50  43.1 MiB/s     2.5", Style::new()),
            ]),
            Line::from(vec![
                Span::styled("🡅 ", Style::new().fg(Color::LightGreen)),
                Span::styled(
                    "also-the-minecraft-movie                7.2 KiB  ",
                    Style::new(),
                ),
                Span::styled("    ", Style::new().fg(Color::Black).bg(Color::LightGreen)),
                Span::styled("30%     ", Style::new().fg(Color::Black).bg(Color::Gray)),
                Span::styled("  Seeding  0/250    4/7    7.9 MiB/s   999.7", Style::new()),
            ]),
        ])
        .render(canvas, buf);
    }

    fn render_log(&self, area: Rect, buf: &mut Buffer) {
        let mut next_line = Rect { height: 1, ..area };

        let visible_lines = if self.logtext.len() > area.height as usize {
            &self.logtext[self.logtext.len() - area.height as usize..]
        } else {
            &self.logtext
        };

        for l in visible_lines {
            Span::raw(l).render(next_line, buf);
            next_line.y += 1;
        }

        // add scrollbar
    }

    fn render_peers(&self, _area: Rect, _buf: &mut Buffer) {}
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let horizontal = Layout::horizontal([Constraint::Min(0), Constraint::Length(2)]);
        let [tab_list_area, _] = horizontal.areas(area);

        let block = Block::default()
            .title(Line::from(" 417 BitTorrent ".bold()).centered())
            .title_bottom("meow :3")
            .borders(Borders::ALL)
            .border_set(border::ROUNDED);

        let inner_area = block.inner(area);
        block.render(area, buf);

        let selected = Style::new().fg(Color::Black).bg(Color::LightBlue);
        let default_style = Style::new();

        Line::from_iter(
            [
                Span::styled(
                    " Downloads ",
                    if self.selected_tab == AppTab::Downloads {
                        selected
                    } else {
                        default_style
                    },
                ),
                Span::styled(
                    " Peers ",
                    if self.selected_tab == AppTab::Peers {
                        selected
                    } else {
                        default_style
                    },
                ),
                Span::styled(
                    " Log ",
                    if self.selected_tab == AppTab::Log {
                        selected
                    } else {
                        default_style
                    },
                ),
            ]
            .into_iter(),
        )
        .right_aligned()
        .render(tab_list_area, buf);

        match self.selected_tab {
            AppTab::Downloads => self.render_downloads(inner_area, buf),
            AppTab::Peers => self.render_peers(inner_area, buf),
            AppTab::Log => self.render_log(inner_area, buf),
        }
    }
}
