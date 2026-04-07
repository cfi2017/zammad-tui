use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::{App, MessageKind};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(4), // Ticket header
        Constraint::Min(5),    // Articles
        Constraint::Length(1), // Status bar
    ])
    .split(f.area());

    draw_ticket_header(f, app, chunks[0]);
    draw_articles(f, app, chunks[1]);
    draw_status_bar(f, app, chunks[2]);
}

fn draw_ticket_header(f: &mut Frame, app: &App, area: Rect) {
    let ticket = match &app.current_ticket {
        Some(t) => t,
        None => return,
    };

    let state = app.state_name(ticket.state_id);
    let priority = app.priority_name(ticket.priority_id);
    let owner = if ticket.owner_id == 0 {
        "-".to_string()
    } else {
        app.user_name(ticket.owner_id)
    };
    let group = app.group_name(ticket.group_id);
    let customer = app.user_name(ticket.customer_id);

    let lines = vec![
        Line::from(vec![
            Span::styled("State: ", Style::default().fg(Color::Gray)),
            Span::styled(state, Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled("Priority: ", Style::default().fg(Color::Gray)),
            Span::styled(priority, Style::default().fg(Color::Magenta)),
            Span::raw("  "),
            Span::styled("Group: ", Style::default().fg(Color::Gray)),
            Span::styled(group, Style::default().fg(Color::Blue)),
        ]),
        Line::from(vec![
            Span::styled("Owner: ", Style::default().fg(Color::Gray)),
            Span::styled(&owner, Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled("Customer: ", Style::default().fg(Color::Gray)),
            Span::styled(customer, Style::default().fg(Color::White)),
        ]),
    ];

    let title = format!(" #{} {} ", ticket.number, ticket.title);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_style(Style::default().add_modifier(Modifier::BOLD));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn draw_articles(f: &mut Frame, app: &App, area: Rect) {
    if app.articles.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" No articles ");
        f.render_widget(block, area);
        return;
    }

    let content_width = area.width.saturating_sub(2) as usize;
    let visible_height = area.height.saturating_sub(2) as usize;

    let mut lines: Vec<Line> = Vec::new();

    for (idx, article) in app.articles.iter().enumerate() {
        let is_selected = idx == app.selected_article;

        // Article header
        let from = article.from.as_deref().unwrap_or("?");
        let date = app.format_time(&article.created_at);
        let article_type = article.article_type.as_deref().unwrap_or("note");
        let internal_marker = if article.internal { " [int]" } else { "" };
        let att_count = article.attachments.len();
        let att_marker = if att_count > 0 {
            format!(" [{att_count} att.]")
        } else {
            String::new()
        };

        // Separator line
        let sep_char = if is_selected { '═' } else { '─' };
        let header_label = format!(
            " [{}/{}] {from} ({article_type}{internal_marker}) {date}{att_marker} ",
            idx + 1,
            app.articles.len(),
        );
        let sep_len = content_width.saturating_sub(header_label.len());
        let left_sep = sep_char.to_string().repeat(2);
        let right_sep = sep_char.to_string().repeat(sep_len.saturating_sub(2));

        let header_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        lines.push(Line::from(Span::styled(
            format!("{left_sep}{header_label}{right_sep}"),
            header_style,
        )));

        if is_selected {
            // Recipients
            if let Some(to) = &article.to {
                lines.push(Line::from(vec![
                    Span::styled("  To: ", Style::default().fg(Color::Gray)),
                    Span::raw(to.as_str()),
                ]));
            }
            if let Some(cc) = &article.cc
                && !cc.is_empty()
            {
                lines.push(Line::from(vec![
                    Span::styled("  Cc: ", Style::default().fg(Color::Gray)),
                    Span::raw(cc.as_str()),
                ]));
            }

            // Attachments inline
            if !article.attachments.is_empty() {
                let att_spans: Vec<Span> = std::iter::once(Span::styled(
                    "  Attachments: ",
                    Style::default().fg(Color::Gray),
                ))
                .chain(article.attachments.iter().flat_map(|a| {
                    let icon = if a.is_image() { "[img] " } else { "" };
                    vec![
                        Span::styled(
                            format!("{icon}{}", a.filename),
                            Style::default().fg(Color::Green),
                        ),
                        Span::raw("  "),
                    ]
                }))
                .collect();
                lines.push(Line::from(att_spans));
            }

            lines.push(Line::from(""));

            // Image attachment indicators
            for att in &article.attachments {
                if att.is_image() {
                    lines.push(Line::from(Span::styled(
                        format!("  [image: {}]", att.filename),
                        Style::default().fg(Color::Green),
                    )));
                }
            }

            // Article body — with quoted text collapsing
            let body = render_article_body(article, content_width);
            let mut in_quote_block = false;
            let mut quote_count = 0u32;

            for line in body.lines() {
                if is_quoted_line(line) {
                    if !in_quote_block {
                        in_quote_block = true;
                        quote_count = 0;
                    }
                    quote_count += 1;
                } else {
                    if in_quote_block {
                        lines.push(Line::from(Span::styled(
                            format!("  [...{quote_count} quoted lines...]"),
                            Style::default().fg(Color::DarkGray),
                        )));
                        in_quote_block = false;
                    }
                    lines.push(Line::from(format!("  {line}")));
                }
            }
            if in_quote_block {
                lines.push(Line::from(Span::styled(
                    format!("  [...{quote_count} quoted lines...]"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        lines.push(Line::from(""));
    }

    let article_nav = format!(
        " Article {}/{} (n/p/1-9/#: navigate, d: attachments, newest first) ",
        app.selected_article + 1,
        app.articles.len()
    );

    let block = Block::default().borders(Borders::ALL).title(article_nav);

    // Apply scroll offset
    let total_lines = lines.len();
    let max_scroll = total_lines.saturating_sub(visible_height);
    let scroll = app.detail_scroll.min(max_scroll);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));

    f.render_widget(paragraph, area);
}

fn render_article_body(article: &crate::api::types::TicketArticle, width: usize) -> String {
    let content_type = article.content_type.as_deref().unwrap_or("text/plain");

    if content_type.contains("html") {
        html2text::from_read(article.body.as_bytes(), width.saturating_sub(2))
    } else {
        article.body.clone()
    }
}

/// Detect lines that are part of email quoting.
fn is_quoted_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    // Classic ">" quoting
    if trimmed.starts_with('>') {
        return true;
    }
    // "On ... wrote:" pattern (start of a quote block)
    if (trimmed.starts_with("On ") || trimmed.starts_with("Am "))
        && (trimmed.ends_with("wrote:") || trimmed.ends_with("schrieb:"))
    {
        return true;
    }
    false
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
        let help = "j/k:scroll  n/p:article  #:jump  r:reply  N:note  o:links  s:status  d:att.  q:back";
        (help.to_string(), Style::default().fg(Color::DarkGray))
    };

    let paragraph = Paragraph::new(Line::from(vec![Span::styled(text, style)]));
    f.render_widget(paragraph, area);
}
