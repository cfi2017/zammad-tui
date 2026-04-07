use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, Tabs},
};

use crate::app::{App, MessageKind};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::vertical([
        Constraint::Length(3), // Tabs
        Constraint::Min(5),    // Table
        Constraint::Length(1), // Status bar
    ])
    .split(f.area());

    draw_tabs(f, app, chunks[0]);
    draw_ticket_table(f, app, chunks[1]);
    draw_status_bar(f, app, chunks[2]);
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    // Available width for tab content (inside borders)
    let inner_width = area.width.saturating_sub(2) as usize;

    // Build a windowed view of tabs that keeps the selected tab visible.
    // Each tab takes its name length + 3 chars for separators/padding.
    let tab_widths: Vec<usize> = app.tabs.iter().map(|t| t.len() + 3).collect();
    let total: usize = tab_widths.iter().sum();

    let (offset, display_select) = if total <= inner_width {
        // All tabs fit
        (0, app.current_tab)
    } else {
        // Find a window of tabs that includes current_tab and fits in inner_width
        let mut start = app.current_tab;
        let mut width = tab_widths[start];
        // Try to include tabs before current
        while start > 0 && width + tab_widths[start - 1] <= inner_width {
            start -= 1;
            width += tab_widths[start];
        }
        // Try to include tabs after current
        let mut end = app.current_tab + 1;
        while end < app.tabs.len() && width + tab_widths[end] <= inner_width {
            width += tab_widths[end];
            end += 1;
        }
        (start, app.current_tab - start)
    };

    let visible_tabs: Vec<Line> = app.tabs[offset..]
        .iter()
        .map(|t| Line::from(t.as_str()))
        .collect();

    let title = format!(
        " Zammad TUI [{}/{}] (t:select) ",
        app.current_tab + 1,
        app.tabs.len()
    );

    let tabs = Tabs::new(visible_tabs)
        .block(Block::default().borders(Borders::ALL).title(title))
        .select(display_select)
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tabs, area);
}

fn draw_ticket_table(f: &mut Frame, app: &mut App, area: Rect) {
    let header = Row::new(vec!["#", "Title", "State", "Priority", "Owner", "Updated"])
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .bottom_margin(1);

    let tickets = app.filtered_tickets();
    let rows: Vec<Row> = tickets
        .iter()
        .map(|t| {
            let state = app.state_name(t.state_id).to_string();
            let priority = app.priority_name(t.priority_id).to_string();
            let owner = if t.owner_id == 0 {
                "-".to_string()
            } else {
                app.user_name(t.owner_id)
            };
            let updated = app.format_time(&t.updated_at);

            Row::new(vec![
                t.number.clone(),
                truncate(&t.title, 50),
                state,
                priority,
                truncate(&owner, 20),
                updated,
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(8),
        Constraint::Min(20),
        Constraint::Length(22),
        Constraint::Length(12),
        Constraint::Length(22),
        Constraint::Length(18),
    ];

    let title = if app.search_mode {
        format!(" Search: {}_ ", app.search_query)
    } else {
        let filter = if app.hide_closed {
            " (hiding closed)"
        } else {
            ""
        };
        let dir = if app.sort_ascending { "↑" } else { "↓" };
        format!(
            " {} ticket(s){} [sort: {}{}] ",
            tickets.len(),
            filter,
            app.sort_field.label(),
            dir,
        )
    };

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(table, area, &mut app.ticket_list_state);
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let (text, style) = if let Some((ref msg, kind)) = app.message {
        let color = match kind {
            MessageKind::Info => Color::Green,
            MessageKind::Error => Color::Red,
        };
        (msg.clone(), Style::default().fg(color))
    } else if app.loading {
        ("Loading...".to_string(), Style::default().fg(Color::Yellow))
    } else {
        let help = "j/k:nav  Enter:open  Tab:queue  o/O:sort  f:filter  /:search  R:refresh  ?:help  q:quit";
        (help.to_string(), Style::default().fg(Color::DarkGray))
    };

    let paragraph = Paragraph::new(Line::from(vec![Span::styled(text, style)]));
    f.render_widget(paragraph, area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
