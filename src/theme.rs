use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    pub root: Style,
    pub selected: Style,
    pub popup: Style,
    pub popup_cursor: Style,
    pub popup_focused: Style,
    pub popup_selected: Style,
}

pub const THEME: Theme = Theme {
    root: Style::new(),
    selected: Style::new().fg(Color::Black).bg(Color::LightBlue),
    popup: Style::new().fg(Color::White),
    popup_cursor: Style::new().fg(Color::Black).bg(Color::White),
    popup_focused: Style::new().fg(Color::White),
    popup_selected: Style::new().fg(Color::Black).bg(Color::LightBlue),
};
