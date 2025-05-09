use std::net::{IpAddr, Ipv4Addr};

use crate::theme::THEME;
use crossterm::event::KeyCode;
use local_ip_address::local_ip;
use ratatui::{
    layout::Flex,
    prelude::*,
    widgets::{Block, BorderType, Clear, Paragraph, Wrap},
};

#[derive(Default)]
pub struct TextEntry {
    text: String,
    cursor_pos: usize,
    max_len: usize,
}

impl TextEntry {
    pub fn new() -> Self {
        TextEntry {
            text: String::new(),
            cursor_pos: 0,
            max_len: usize::MAX,
        }
    }

    pub fn with_text(text: String) -> Self {
        let cursor_pos = text.len();
        TextEntry {
            text,
            cursor_pos,
            max_len: usize::MAX,
        }
    }

    pub fn with_max(mut self, len: usize) -> Self {
        self.max_len = len;
        self
    }

    pub fn take(&mut self) -> String {
        self.move_cursor_home();
        std::mem::take(&mut self.text)
    }

    pub fn set_text(&mut self, new_text: String) {
        self.text = new_text;
        self.cursor_pos = self.text.len();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.move_cursor_home();
    }

    pub fn get_str(&self) -> &str {
        self.text.as_str()
    }

    pub fn get_cursor_pos(&self) -> usize {
        self.cursor_pos
    }

    fn byte_index(&self) -> usize {
        self.text
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.cursor_pos)
            .unwrap_or(self.text.len())
    }

    pub fn move_cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor_pos = self.text.len();
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.text.len() {
            self.cursor_pos += 1;
        }
    }

    pub fn insert(&mut self, c: char) {
        if self.cursor_pos < self.max_len {
            self.text.insert(self.byte_index(), c);
            self.move_cursor_right();
        }
    }

    pub fn remove(&mut self) {
        if self.text.is_empty() {
            return;
        }

        // stops backspace from acting like del when at the beginning of the string
        if self.cursor_pos == 0 {
            return;
        }

        self.move_cursor_left();
        self.text.remove(self.byte_index());
    }
}

#[derive(Default, PartialEq)]
pub enum PopupStatus {
    InUse,
    Canceled,
    Confirmed,
    #[default]
    Closed,
}

#[derive(Default)]
pub struct TextEntryPopup {
    pub text_field: TextEntry,
    pub title: String,
    pub status: PopupStatus,
    pub max_lines: u16,
}

impl TextEntryPopup {
    pub fn handle_input(&mut self, key: KeyCode) {
        match key {
            KeyCode::Enter => self.confirm(),
            KeyCode::Esc => self.cancel(),
            KeyCode::Char(c) => self.text_field.insert(c),
            KeyCode::Backspace => self.text_field.remove(),
            KeyCode::Left => self.text_field.move_cursor_left(),
            KeyCode::Right => self.text_field.move_cursor_right(),
            _ => {}
        }
    }

    pub fn new(title: String, max_lines: u16) -> Self {
        TextEntryPopup {
            text_field: TextEntry::new(),
            title,
            status: PopupStatus::default(),
            max_lines,
        }
    }

    fn confirm(&mut self) {
        self.status = PopupStatus::Confirmed;
    }

    fn cancel(&mut self) {
        self.status = PopupStatus::Canceled;
    }

    pub fn close(&mut self) {
        self.status = PopupStatus::Closed;
    }

    pub fn show(&mut self) {
        self.status = PopupStatus::InUse;
    }

    pub fn reset(&mut self) {
        self.close();
        self.text_field.clear();
    }

    pub fn take(&mut self) -> String {
        std::mem::take(&mut self.text_field.take())
    }
}

impl Widget for &TextEntryPopup {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let vertical = Layout::vertical([self.max_lines + 2]).flex(Flex::Center);
        let horizontal = Layout::horizontal([60]).flex(Flex::Center);
        let [area] = vertical.areas(area);
        let [area] = horizontal.areas(area);

        let window = Block::bordered()
            .style(THEME.popup)
            .border_style(THEME.popup)
            .border_type(BorderType::Rounded)
            .title_top(self.title.as_str())
            .title_bottom(Line::raw(" [Esc] to Cancel [Enter] to Confirm ").right_aligned());

        let win_area = window.inner(area);
        Clear.render(win_area, buf);
        window.render(area, buf);

        Paragraph::new(self.text_field.get_str())
            .wrap(Wrap { trim: true })
            .style(THEME.popup_focused)
            .render(win_area, buf);

        let cursor_pos = self.text_field.get_cursor_pos() as u16;
        buf[(win_area.x + cursor_pos % 58, win_area.y + cursor_pos / 58)]
            .set_style(THEME.popup_cursor);
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationField {
    #[default]
    Yes,
    No,
}

impl ConfirmationField {
    pub fn cycle_next(&mut self) {
        *self = match self {
            ConfirmationField::No => ConfirmationField::Yes,
            ConfirmationField::Yes => ConfirmationField::No,
        }
    }
}

pub struct ConfirmationPopup {
    pub title: String,
    pub body: String,
    pub status: PopupStatus,

    selected_field: ConfirmationField,
}

impl ConfirmationPopup {
    pub fn handle_input(&mut self, key: KeyCode) {
        match key {
            KeyCode::Tab => {
                self.selected_field.cycle_next();
            }
            KeyCode::BackTab => {
                self.selected_field.cycle_next();
            }
            KeyCode::Char('y') | KeyCode::Char('q') => {
                self.selected_field = ConfirmationField::Yes;
                self.status = PopupStatus::Confirmed;
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.selected_field = ConfirmationField::No;
                self.status = PopupStatus::Confirmed;
            }
            KeyCode::Enter => {
                self.status = PopupStatus::Confirmed;
            }
            //KeyCode::Char('q') => {},
            _ => {}
        }
    }

    pub fn new(new_title: String, new_body: String) -> ConfirmationPopup {
        ConfirmationPopup {
            selected_field: ConfirmationField::default(),
            title: new_title,
            body: new_body,
            status: PopupStatus::Closed,
        }
    }

    pub fn show(&mut self) {
        self.selected_field = ConfirmationField::default();
        self.status = PopupStatus::InUse;
    }

    pub fn close(&mut self) {
        self.status = PopupStatus::Closed;
    }

    pub fn decision(&self) -> bool {
        self.selected_field == ConfirmationField::Yes
        //match self.selected_field {
        //    ConfirmationField::No => false,
        //    ConfirmationField::Yes => true,
        //}
    }
}

impl Widget for &ConfirmationPopup {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let vertical = Layout::vertical([5]).flex(Flex::Center);
        let horizontal = Layout::horizontal([45]).flex(Flex::Center);
        let [area] = vertical.areas(area);
        let [area] = horizontal.areas(area);

        let window = Block::bordered()
            .style(THEME.popup)
            .border_style(THEME.popup)
            .border_type(BorderType::Rounded)
            .title(Span::from(&self.title));

        let win_area = window.inner(area);
        Clear.render(win_area, buf);
        window.render(area, buf);

        let vertical = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ]);
        let [body_area, _gap, button_area] = vertical.areas(win_area);

        Paragraph::new(self.body.as_str())
            .style(THEME.popup)
            .alignment(Alignment::Center)
            .render(body_area, buf);

        Line::from(vec![
            Span::from(" No ").style(if self.selected_field == ConfirmationField::No {
                THEME.popup_selected
            } else {
                THEME.popup
            }),
            Span::from("               "),
            Span::from(" Yes ").style(if self.selected_field == ConfirmationField::Yes {
                THEME.popup_selected
            } else {
                THEME.popup
            }),
        ])
        .centered()
        .render(button_area, buf);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum IpMode {
    #[default]
    Auto,
    V4,
    V6,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum OpenTorrentField {
    #[default]
    Path,
    Ip,
    Port,
    IpMode,
    Compact,
}

impl OpenTorrentField {
    fn from_u8(num: u8) -> Self {
        match num {
            0 => Self::Path,
            1 => Self::Ip,
            2 => Self::Port,
            3 => Self::IpMode,
            4 => Self::Compact,
            _ => panic!(),
        }
    }

    fn next(self) -> Self {
        Self::from_u8((self as u8).wrapping_add(1) % 5)
    }

    fn prev(self) -> Self {
        Self::from_u8(match (self as u8).wrapping_sub(1) {
            255 => 5,
            n => n,
        })
    }
}

pub struct OpenTorrentResult {
    pub path: String,
    pub ip: IpAddr,
    pub port: u16,
    pub compact: bool,
}

pub struct OpenTorrentPopup {
    pub text_field: TextEntry,
    pub title: String,
    pub status: PopupStatus,
    pub max_lines: u16,
    pub port: TextEntry,
    pub ip: TextEntry,
    ip_mode: IpMode,
    pub compact: bool,
    selected: OpenTorrentField,
}

impl OpenTorrentPopup {
    pub fn handle_input(&mut self, key: KeyCode) {
        match key {
            KeyCode::Enter => self.confirm(),
            KeyCode::Esc => self.cancel(),
            KeyCode::Tab => self.selected = self.selected.next(),
            KeyCode::BackTab => self.selected = self.selected.prev(),
            _ => {}
        }

        match self.selected {
            OpenTorrentField::Path => match key {
                KeyCode::Char(c) => self.text_field.insert(c),
                KeyCode::Backspace => self.text_field.remove(),
                KeyCode::Left => self.text_field.move_cursor_left(),
                KeyCode::Right => self.text_field.move_cursor_right(),
                _ => {}
            },
            OpenTorrentField::Ip => match key {
                KeyCode::Char(c) => self.ip.insert(c),
                KeyCode::Backspace => self.ip.remove(),
                KeyCode::Left => self.ip.move_cursor_left(),
                KeyCode::Right => self.ip.move_cursor_right(),
                _ => {}
            },
            OpenTorrentField::Port => match key {
                KeyCode::Char(c) => self.port.insert(c),
                KeyCode::Backspace => self.port.remove(),
                KeyCode::Left => self.port.move_cursor_left(),
                KeyCode::Right => self.port.move_cursor_right(),
                _ => {}
            },
            OpenTorrentField::IpMode => match key {
                KeyCode::Char('a') => self.ip_mode = IpMode::Auto,
                KeyCode::Char('4') => self.ip_mode = IpMode::V4,
                KeyCode::Char('6') => self.ip_mode = IpMode::V6,
                _ => {}
            },
            OpenTorrentField::Compact => match key {
                KeyCode::Char('t') => self.compact = true,
                KeyCode::Char('f') => self.compact = false,
                _ => {}
            },
        }
    }

    pub fn new(title: String, max_lines: u16) -> Self {
        OpenTorrentPopup {
            text_field: TextEntry::new(),
            title,
            status: PopupStatus::default(),
            max_lines,
            port: TextEntry::with_text("45123".to_owned()).with_max(5),
            ip: TextEntry::with_text(
                local_ip()
                    .unwrap_or(Ipv4Addr::new(127, 0, 0, 1).into())
                    .to_string(),
            )
            .with_max(39),
            ip_mode: IpMode::default(),
            compact: true,
            selected: OpenTorrentField::default(),
        }
    }

    fn confirm(&mut self) {
        self.status = PopupStatus::Confirmed;
    }

    fn cancel(&mut self) {
        self.status = PopupStatus::Canceled;
    }

    fn get_style(&self, field: OpenTorrentField) -> Style {
        if self.selected == field {
            THEME.popup_selected
        } else {
            THEME.popup
        }
    }

    fn get_cursor_style(&self, field: OpenTorrentField) -> Style {
        if self.selected == field {
            THEME.popup_cursor
        } else {
            THEME.popup
        }
    }

    pub fn close(&mut self) {
        self.status = PopupStatus::Closed;
    }

    pub fn show(&mut self) {
        self.status = PopupStatus::InUse;
    }

    pub fn reset(&mut self) {
        self.close();
        self.text_field.clear();
        self.ip.set_text(
            local_ip()
                .unwrap_or(Ipv4Addr::new(127, 0, 0, 1).into())
                .to_string(),
        );
        self.port.set_text("45123".to_owned());
        self.ip_mode = IpMode::default();
        self.compact = true;
        self.selected = OpenTorrentField::default();
    }

    pub fn take(&mut self) -> Result<OpenTorrentResult, ()> {
        Ok(OpenTorrentResult {
            path: std::mem::take(&mut self.text_field.take()),
            ip: std::mem::take(&mut self.ip.take()).parse().or(Err(()))?,
            port: std::mem::take(&mut self.port.take()).parse().or(Err(()))?,
            compact: self.compact,
        })
    }
}

impl Widget for &OpenTorrentPopup {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let vertical = Layout::vertical([self.max_lines + 2 + 2]).flex(Flex::Center);
        let horizontal = Layout::horizontal([60]).flex(Flex::Center);
        let [area] = vertical.areas(area);
        let [area] = horizontal.areas(area);

        let window = Block::bordered()
            .style(THEME.popup)
            .border_style(THEME.popup)
            .border_type(BorderType::Rounded)
            .title_top(self.title.as_str())
            .title_bottom(Line::raw(" [Esc] to Cancel [Enter] to Confirm ").right_aligned());

        let win_area = window.inner(area);
        Clear.render(win_area, buf);
        window.render(area, buf);

        let vertical = Layout::vertical([
            Constraint::Length(self.max_lines),
            Constraint::Length(1),
            Constraint::Length(1),
        ]);
        let [text_area, ip_area, bottom_area] = vertical.areas(win_area);
        let horizontal = Layout::horizontal([
            Constraint::Length(13),
            Constraint::Length(12),
            Constraint::Length(16),
        ])
        .flex(Flex::SpaceAround);
        let [port_area, ipmode_area, compact_area] = horizontal.areas(bottom_area);

        Span::raw(format!(" Path: {} ", self.text_field.get_str()).as_str()).render(text_area, buf);
        buf.set_style(text_area, self.get_style(OpenTorrentField::Path));
        let cursor_pos = self.text_field.get_cursor_pos() as u16;
        buf[(text_area.x + cursor_pos + 7, text_area.y)]
            .set_style(self.get_cursor_style(OpenTorrentField::Path));

        Span::raw(format!(" Ip: {} ", self.ip.get_str()).as_str()).render(ip_area, buf);
        buf.set_style(ip_area, self.get_style(OpenTorrentField::Ip));
        let cursor_pos = self.ip.get_cursor_pos() as u16;
        buf[(ip_area.x + cursor_pos + 5, ip_area.y)]
            .set_style(self.get_cursor_style(OpenTorrentField::Ip));

        Span::raw(format!(" Port: {} ", self.port.get_str()).as_str()).render(port_area, buf);
        buf.set_style(port_area, self.get_style(OpenTorrentField::Port));
        let cursor_pos = self.port.get_cursor_pos() as u16;
        buf[(port_area.x + cursor_pos + 7, port_area.y)]
            .set_style(self.get_cursor_style(OpenTorrentField::Port));

        Span::raw(format!(" Mode: {:?} ", self.ip_mode).as_str()).render(ipmode_area, buf);
        buf.set_style(ipmode_area, self.get_style(OpenTorrentField::IpMode));

        Span::raw(format!(" Compact: {} ", self.compact).as_str()).render(compact_area, buf);
        buf.set_style(compact_area, self.get_style(OpenTorrentField::Compact));
    }
}
