use crate::{
    logger::{LogTab, LEVELS},
    metainfo::Info,
    popup::{ConfirmationPopup, OpenTorrentPopup, OpenTorrentResult, PopupStatus},
    theme::THEME,
    torrent::{handle_torrent, Torrent, TorrentInfo, TorrentStatus},
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use futures::{FutureExt, StreamExt};
use log::{debug, error, info, trace};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::*,
    style::Stylize,
    symbols::border,
    text::Line,
    widgets::{Block, Borders, Widget},
    Frame,
};
use std::{fs::File, io::BufWriter, io::Write};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

type Tui = Terminal<CrosstermBackend<std::io::Stdout>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppTab {
    Downloads,
    Peers,
    Log,
}

pub struct App {
    exit: bool,
    event_handler: EventHandler,
    log_tab: LogTab,
    save_window: ConfirmationPopup,
    open_window: OpenTorrentPopup,
    rx: std::sync::mpsc::Receiver<(log::Level, String)>,
    selected_tab: AppTab,
    torrents: Vec<(
        String,
        tokio::task::JoinHandle<()>,
        UnboundedSender<TorrentStatus>,
        UnboundedReceiver<TorrentInfo>,
        TorrentInfo,
    )>,
}

impl App {
    pub fn new(rx: std::sync::mpsc::Receiver<(log::Level, String)>) -> Self {
        Self {
            exit: false,
            event_handler: EventHandler::new(16),
            log_tab: LogTab::new(),
            save_window: ConfirmationPopup::new(
                "".to_owned(),
                "Are you sure you want to quit?".to_owned(),
            ),
            open_window: OpenTorrentPopup::new(" Open new Torrent ".to_owned(), 1),
            rx,
            selected_tab: AppTab::Downloads,
            torrents: Vec::new(),
        }
    }

    /// runs the application's main loop until the user quits
    pub async fn run(&mut self, terminal: &mut Tui) -> Result<(), std::io::Error> {
        while !self.exit {
            terminal.draw(|frame| self.render_frame(frame))?;
            let event = self.event_handler.next().await?;
            self.handle_events(event)?;

            // handle logging
            if let Ok(s) = self.rx.try_recv() {
                self.log_tab.push(s);
            }

            // popup handler
            match self.save_window.status {
                PopupStatus::Closed | PopupStatus::InUse => {}
                PopupStatus::Canceled => {
                    self.save_window.close();
                }
                PopupStatus::Confirmed => {
                    self.save_window.close();
                    self.exit = self.save_window.decision();
                }
            }
            match self.open_window.status {
                PopupStatus::Closed | PopupStatus::InUse => {}
                PopupStatus::Canceled => {
                    self.open_window.reset();
                }
                PopupStatus::Confirmed => {
                    let result = self.open_window.take();
                    self.open_window.reset();
                    self.open_torrent(result);
                }
            }
            // TODO: handle bittorrent stuff

            // TODO: check socket assosciated with each active torrent for information from tracker
            // if new peers have been found or this is the first reponse send that information to
            // worker threads
            for t in &mut self.torrents {
                let Ok(info) = t.3.try_recv() else { continue };
                t.4 = info;
            }

            // TODO: send periodic updates to tracker for each active torrent
        }

        let mut logfile = BufWriter::new(File::create("log.txt")?);
        for (l, t) in &self.log_tab.text {
            logfile.write_all(format!("{}{}", LEVELS[*l as usize - 1], t).as_bytes())?;
        }
        logfile.flush()?;

        Ok(())
    }

    fn try_quit(&mut self) {
        self.save_window.show();
    }

    fn render_frame(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    /// updates the application's state based on user input
    fn handle_events(&mut self, event: Event) -> Result<(), std::io::Error> {
        match event {
            // it's important to check that the event is a key press event as
            // crossterm also emits key release and repeat events on Windows.
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event)?
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<(), std::io::Error> {
        if self.save_window.status == PopupStatus::InUse {
            self.save_window.handle_input(key_event.code);
        } else if self.open_window.status == PopupStatus::InUse {
            self.open_window.handle_input(key_event.code);
        } else {
            match key_event.code {
                KeyCode::Char('q') | KeyCode::Esc => self.try_quit(),
                KeyCode::Char('d') | KeyCode::Char('1') => self.selected_tab = AppTab::Downloads,
                KeyCode::Char('p') | KeyCode::Char('2') => self.selected_tab = AppTab::Peers,
                KeyCode::Char('l') | KeyCode::Char('3') => self.selected_tab = AppTab::Log,
                KeyCode::Char('o') => self.open_window.show(),
                KeyCode::Char('j') => self.log_tab.scroll_down(),
                KeyCode::Char('k') => self.log_tab.scroll_up(),
                _ => {}
            }
        }
        Ok(())
    }

    fn open_torrent(&mut self, result: Result<OpenTorrentResult, ()>) {
        let Ok(torrent) = result else {
            error!("Invalid fields in open");
            return;
        };

        let new_torrent = match Torrent::open(torrent) {
            Ok(f) => f,
            Err(e) => {
                error!("{e}");
                return;
            }
        };
        // send out initial request to
        let info = new_torrent.get_info();
        let name = match &new_torrent.meta_info.info {
            Info::Single(f) => f.name.clone(),
            Info::Multi(f) => f.name.clone(),
        };
        let (tx1, rx1) = tokio::sync::mpsc::unbounded_channel();
        let (tx2, rx2) = tokio::sync::mpsc::unbounded_channel();
        let handle = tokio::spawn(async move {
            if let Err(e) = handle_torrent(new_torrent, tx1, rx2).await {
                error!("{e}");
            }
        });

        self.torrents.push((name, handle, tx2, rx1, info));
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
            " ",
            " Name",
            " Size",
            " Progress",
            " Status",
            " Seeds",
            " Peers",
            " Speed",
            " TODO",
        ];
        let title_bar_areas: [_; 9] = horizontal.areas(title_bar);
        Block::new()
            .style(Style::new().bg(Color::Black))
            .render(title_bar, buf);
        for (i, &t) in columns.iter().enumerate() {
            Span::raw(t).render(title_bar_areas[i], buf);
        }

        for (torrent_name, _, _, _, torrent) in &self.torrents {
            let [_icon, name, size, mut progress, status, _seeds, _peers, _speed, _todo] =
                horizontal.areas(canvas);

            Span::raw(torrent_name).render(name, buf);
            Span::raw(format!("{:?}", torrent.status)).render(status, buf);
            Span::raw(format!("   %{}   ", torrent.progress)).render(progress, buf);
            progress.height = 1;
            progress.width = 10;
            buf.set_style(progress, THEME.not_downloaded);
            progress.width = torrent.progress as u16 / 10;
            buf.set_style(progress, THEME.downloaded);
            Span::raw(convert_to_human(torrent.size)).render(size, buf);

            canvas.y += 1;
        }

        //Paragraph::new(vec![Line::from(vec![
        //    Span::styled("🡅 ", Style::new().fg(Color::LightGreen)),
        //    Span::styled(
        //        "minecraft-movie-but-real-this-time      1.4 GiB  ",
        //        Style::new(),
        //    ),
        //    Span::styled(
        //        "    50",
        //        Style::new().fg(Color::Black).bg(Color::LightGreen),
        //    ),
        //    Span::styled("%     ", Style::new().fg(Color::Black).bg(Color::Gray)),
        //    Span::styled("  Seeding  0/250    4/7    7.9 MiB/s   999.7", Style::new()),
        //])])
        //.render(canvas, buf);
    }

    fn render_peers(&self, _area: Rect, _buf: &mut Buffer) {}

    fn get_tabstyle(&self, tab: AppTab) -> Style {
        if self.selected_tab == tab {
            THEME.selected
        } else {
            THEME.root
        }
    }
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

        Line::from_iter([
            Span::styled(" ", self.get_tabstyle(AppTab::Downloads)),
            Span::styled("D", self.get_tabstyle(AppTab::Downloads).underlined()),
            Span::styled("ownloads ", self.get_tabstyle(AppTab::Downloads)),
            Span::styled(" ", self.get_tabstyle(AppTab::Peers)),
            Span::styled("P", self.get_tabstyle(AppTab::Peers).underlined()),
            Span::styled("eers ", self.get_tabstyle(AppTab::Peers)),
            Span::styled(" ", self.get_tabstyle(AppTab::Log)),
            Span::styled("L", self.get_tabstyle(AppTab::Log).underlined()),
            Span::styled("og ", self.get_tabstyle(AppTab::Log)),
        ])
        .right_aligned()
        .render(tab_list_area, buf);

        match self.selected_tab {
            AppTab::Downloads => self.render_downloads(inner_area, buf),
            AppTab::Peers => self.render_peers(inner_area, buf),
            AppTab::Log => self.log_tab.render(inner_area, buf),
        }

        if self.save_window.status == PopupStatus::InUse {
            self.save_window.render(area, buf);
        } else if self.open_window.status == PopupStatus::InUse {
            self.open_window.render(area, buf);
        }
    }
}

const UNITS: [&'static str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];

// TODO: add fractional part (probably using floats) and store output to reduce cost
fn convert_to_human(bytes: u64) -> String {
    for i in 1..=6 {
        if bytes >> (i * 10) == 0 {
            return format!("{} {}", bytes >> ((i - 1) * 10), UNITS[i - 1]);
        }
    }
    format!("{} {}", bytes >> (6 * 10), UNITS[6])
}

#[derive(Clone, Copy, Debug)]
pub enum Event {
    Error,
    Tick,
    Key(KeyEvent),
}

#[derive(Debug)]
pub struct EventHandler {
    _tx: UnboundedSender<Event>,
    rx: UnboundedReceiver<Event>,
    _task: Option<JoinHandle<()>>,
}

impl EventHandler {
    pub fn new(tick_rate: u64) -> Self {
        let tick_rate = std::time::Duration::from_millis(tick_rate);

        let (tx, rx) = mpsc::unbounded_channel();
        let _tx = tx.clone();

        let task = tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();
            let mut interval = tokio::time::interval(tick_rate);
            loop {
                let delay = interval.tick();
                let crossterm_event = reader.next().fuse();
                tokio::select! {
                    maybe_event = crossterm_event => {
                        match maybe_event {
                            Some(Ok(crossterm::event::Event::Key(key))) => {
                                if key.kind == crossterm::event::KeyEventKind::Press {
                                    tx.send(Event::Key(key)).unwrap();
                                }
                            },
                            Some(Err(_)) => {
                                tx.send(Event::Error).unwrap();
                            },
                            Some(Ok(_)) | None => {},
                        }
                    },
                    _ = delay => {
                      tx.send(Event::Tick).unwrap();
                    },
                }
            }
        });

        Self {
            _tx,
            rx,
            _task: Some(task),
        }
    }

    pub async fn next(&mut self) -> Result<Event, std::io::Error> {
        self.rx.recv().await.ok_or(std::io::ErrorKind::Other.into())
    }
}
