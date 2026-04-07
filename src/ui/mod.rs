mod popup;
mod ticket_detail;
mod ticket_list;

use ratatui::Frame;

use crate::app::{App, View};

pub fn draw(f: &mut Frame, app: &mut App) {
    match app.view {
        View::TicketList => ticket_list::draw(f, app),
        View::TicketDetail => ticket_detail::draw(f, app),
    }

    // Draw modal overlay if present
    if app.modal.is_some() {
        popup::draw(f, app);
    }
}
