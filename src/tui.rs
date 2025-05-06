use crate::{
    logger::LogTab,
    metainfo::{Info, MetaInfo, SingleFileInfo},
    torrent::{Torrent, TorrentStatus},
};
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use log::{error, info, trace};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::*,
    style::Stylize,
    symbols::border,
    text::Line,
    widgets::{Block, Borders, Paragraph, Widget},
    Frame,
};
use regex::Regex;
use std::{
    fs::File,
    io::{Read, Stdout},
    net::{SocketAddr, ToSocketAddrs},
};

type Tui = Terminal<CrosstermBackend<Stdout>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppTab {
    Downloads,
    Peers,
    Log,
}

pub struct App {
    exit: bool,
    hostname_regex: Regex,
    log_tab: LogTab,
    rx: std::sync::mpsc::Receiver<(log::Level, String)>,
    selected_tab: AppTab,
    torrents: Vec<Torrent>,
}

impl App {
    pub fn new(rx: std::sync::mpsc::Receiver<(log::Level, String)>) -> Self {
        Self {
            exit: false,
            hostname_regex: Regex::new(r"(?P<proto>https?|udp)://(?P<name>[^/]+)").unwrap(),
            log_tab: LogTab::new(),
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
                self.log_tab.push(s);
            }

            // TODO: handle bittorrent stuff

            // TODO: check socket assosciated with each active torrent for information from tracker
            // if new peers have been found or this is the first reponse send that information to
            // worker threads.

            // TODO: send periodic updates to tracker for each active torrent
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
            KeyCode::Char('o') => self.open_torrent("./ubuntu-25.04-desktop-amd64.iso.torrent"),
            KeyCode::Char('j') => self.log_tab.scroll_down(),
            KeyCode::Char('k') => self.log_tab.scroll_up(),
            _ => {}
        }
        Ok(())
    }

    fn open_torrent(&mut self, path: &str) {
        let mut file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to open torrent file {:?}", e);
                return;
            }
        };

        let mut data = Vec::new();
        let bytes_read = file.read_to_end(&mut data);
        info!("open_torrent() read {:?} bytes", bytes_read);

        let new_meta: MetaInfo = match serde_bencode::from_bytes(&data) {
            Ok(t) => t,
            Err(e) => {
                error!("{}", e);
                return;
            }
        };

        info!("tracker addr {}", new_meta.announce);
        let Some(caps) = self.hostname_regex.captures(&new_meta.announce) else {
            error!("Unable to parse tracker URL");
            return;
        };
        let proto = caps.name("proto").unwrap();
        if proto.as_str() == "udp" {
            error!("UDP trackers are not supported");
            return;
        }

        let hostname = caps.name("name").unwrap();
        let ip = match format!("{}:80", hostname.as_str()).to_socket_addrs() {
            Ok(mut ip_iter) => {
                let ip_opt = ip_iter.next();
                if let Some(ip) = ip_opt {
                    ip
                } else {
                    error!("Unable to resolve Tracker URL");
                    return;
                }
            }
            Err(e) => {
                error!("{}", e);
                return;
            }
        };

        let new_torrent = match new_meta.info {
            Info::Single(_) => Torrent {
                status: TorrentStatus::Waiting,
                meta_info: new_meta,
                tracker_addr: ip,
            },
            Info::Multi(_) => {
                error!("Multifile mode currently not supported");
                return;
            }
        };

        // send out initial request to

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

            match &torrent.meta_info.info {
                Info::Multi(_) => {}
                Info::Single(f) => {
                    Span::raw(&f.name).render(name, buf);
                    Span::raw(format!("{}", f.length)).render(size, buf);
                }
            }

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

        Line::from_iter([
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
        ])
        .right_aligned()
        .render(tab_list_area, buf);

        match self.selected_tab {
            AppTab::Downloads => self.render_downloads(inner_area, buf),
            AppTab::Peers => self.render_peers(inner_area, buf),
            AppTab::Log => self.log_tab.render(inner_area, buf),
        }
    }
}
