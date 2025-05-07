use crate::theme::THEME;
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Flex, Offset},
    prelude::*,
    widgets::{
        block::{Position, Title},
        Block, BorderType, Clear, Paragraph, Wrap,
    },
};

#[derive(Default)]
pub struct TextEntry {
    text: String,
    cursor_pos: usize,
}

impl TextEntry {
    pub fn new() -> Self {
        TextEntry {
            text: String::new(),
            cursor_pos: 0,
        }
    }

    pub fn take(&mut self) -> String {
        std::mem::take(&mut self.text)
    }

    pub fn set_text(&mut self, new_text: String) {
        self.text = new_text;
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
        self.text.insert(self.byte_index(), c);
        self.move_cursor_right();
    }

    pub fn remove(&mut self) {
        if self.text.len() == 0 {
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
            .title_bottom(
                Line::raw(format!(" [Esc] to Cancel [Enter] to Confirm ")).right_aligned(),
            );

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
