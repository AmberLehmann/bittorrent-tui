use ratatui::style::{Color, Style};

pub struct Theme {
    pub root: Style,
    pub selected: Style,
    pub popup: Style,
    pub popup_cursor: Style,
    pub popup_focused: Style,
    pub popup_selected: Style,
    pub not_downloaded: Style,
    pub downloaded: Style,
}

pub const THEME: Theme = Theme {
    root: Style::new(),
    selected: Style::new().fg(Color::Black).bg(Color::LightBlue),
    popup: Style::new().fg(Color::White),
    popup_cursor: Style::new().fg(Color::LightBlue).bg(Color::Black),
    popup_focused: Style::new().fg(Color::White),
    popup_selected: Style::new().fg(Color::Black).bg(Color::LightBlue),
    not_downloaded: Style::new().fg(Color::Black).bg(Color::Gray),
    downloaded: Style::new().fg(Color::Black).bg(Color::LightGreen),
};
