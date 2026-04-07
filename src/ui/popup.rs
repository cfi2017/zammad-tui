use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::app::{App, Modal};

pub fn draw(f: &mut Frame, app: &App) {
    let modal = match &app.modal {
        Some(m) => m,
        None => return,
    };

    match modal {
        Modal::StatusSelect {
            items,
            filtered,
            filter,
            selected,
        } => {
            draw_select_modal(
                f,
                "Select Status",
                filter,
                &filtered
                    .iter()
                    .map(|&i| items[i].name.as_str())
                    .collect::<Vec<_>>(),
                *selected,
            );
        }
        Modal::AssigneeSelect {
            items,
            filtered,
            filter,
            selected,
        } => {
            let names: Vec<String> = filtered.iter().map(|&i| items[i].display_name()).collect();
            let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
            draw_select_modal(f, "Select Assignee", filter, &name_refs, *selected);
        }
        Modal::PrioritySelect {
            items,
            filtered,
            filter,
            selected,
        } => {
            draw_select_modal(
                f,
                "Select Priority",
                filter,
                &filtered
                    .iter()
                    .map(|&i| items[i].name.as_str())
                    .collect::<Vec<_>>(),
                *selected,
            );
        }
        Modal::GroupSelect {
            items,
            filtered,
            filter,
            selected,
        } => {
            draw_select_modal(
                f,
                "Select Queue",
                filter,
                &filtered
                    .iter()
                    .map(|&i| items[i].1.as_str())
                    .collect::<Vec<_>>(),
                *selected,
            );
        }
        Modal::AttachmentSelect { items, selected } => {
            draw_attachment_modal(f, items, *selected);
        }
        Modal::LinkSelect { items, selected } => {
            draw_link_modal(f, items, *selected);
        }
        Modal::ArticleJump {
            items,
            filtered,
            filter,
            selected,
        } => {
            draw_select_modal(
                f,
                "Jump to Article",
                filter,
                &filtered
                    .iter()
                    .map(|&i| items[i].1.as_str())
                    .collect::<Vec<_>>(),
                *selected,
            );
        }
        Modal::Help => {
            draw_help_modal(f);
        }
    }
}

fn draw_select_modal(f: &mut Frame, title: &str, filter: &str, items: &[&str], selected: usize) {
    let area = centered_rect(50, 60, f.area());

    // Clear background
    f.render_widget(Clear, area);

    let chunks = Layout::vertical([
        Constraint::Length(3), // Filter input
        Constraint::Min(3),    // List
    ])
    .split(area);

    // Filter input
    let filter_text = if filter.is_empty() {
        "Type to filter...".to_string()
    } else {
        filter.to_string()
    };
    let filter_style = if filter.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };
    let input = Paragraph::new(Line::from(Span::styled(filter_text, filter_style))).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {title} "))
            .title_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
    );
    f.render_widget(input, chunks[0]);

    // Item list
    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let style = if i == selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
                    .bg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(*name, style)))
        })
        .collect();

    let list = List::new(list_items).block(Block::default().borders(Borders::ALL).title(format!(
        " {}/{} ",
        items.len(),
        items.len()
    )));

    f.render_widget(list, chunks[1]);
}

fn draw_attachment_modal(
    f: &mut Frame,
    items: &[(usize, usize, String)],
    selected: usize,
) {
    let area = centered_rect(60, 50, f.area());
    f.render_widget(Clear, area);

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, (_, _, label))| {
            let style = if i == selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
                    .bg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(label.as_str(), style)))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Attachments (Enter: download/view) ")
        .title_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    let list = List::new(list_items).block(block);
    f.render_widget(list, area);
}

fn draw_link_modal(f: &mut Frame, items: &[String], selected: usize) {
    let area = centered_rect(70, 50, f.area());
    f.render_widget(Clear, area);

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, url)| {
            let style = if i == selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
                    .bg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Cyan)
            };
            ListItem::new(Line::from(Span::styled(url.as_str(), style)))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Open Link (Enter: open in browser) ")
        .title_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    let list = List::new(list_items).block(block);
    f.render_widget(list, area);
}

fn draw_help_modal(f: &mut Frame) {
    let area = centered_rect(60, 70, f.area());
    f.render_widget(Clear, area);

    let help_text = vec![
        Line::from(Span::styled(
            "Ticket List",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  j/k, Up/Down  Navigate tickets"),
        Line::from("  Enter         Open ticket"),
        Line::from("  Tab/S-Tab     Next/previous queue"),
        Line::from("  t             Select queue"),
        Line::from("  o             Cycle sort field"),
        Line::from("  O             Toggle sort direction"),
        Line::from("  f             Toggle closed filter"),
        Line::from("  /             Search tickets"),
        Line::from("  R             Refresh"),
        Line::from("  g/G           Go to first/last"),
        Line::from("  q, Esc        Quit"),
        Line::from(""),
        Line::from(Span::styled(
            "Ticket Detail",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  j/k, Up/Down  Scroll content"),
        Line::from("  n/p           Next/prev article (newest first)"),
        Line::from("  1-9           Jump to article #"),
        Line::from("  #/J           Jump to article (search)"),
        Line::from("  r             Reply (opens $EDITOR)"),
        Line::from("  N             Add internal note ($EDITOR)"),
        Line::from("  s             Change status"),
        Line::from("  a             Change assignee"),
        Line::from("  P             Change priority"),
        Line::from("  o             Open links in browser"),
        Line::from("  d/v/i         Browse attachments"),
        Line::from("  R             Refresh"),
        Line::from("  q, Esc        Back to list"),
        Line::from(""),
        Line::from(Span::styled(
            "Selection Popup",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  Type          Filter options"),
        Line::from("  j/k, Up/Down  Navigate"),
        Line::from("  Enter         Confirm selection"),
        Line::from("  Esc           Cancel"),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help ")
        .title_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

/// Create a centered rect of given percentage width and height.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let [area] = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(Flex::Center)
        .areas(r);
    let [area] = Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .areas(area);
    area
}
