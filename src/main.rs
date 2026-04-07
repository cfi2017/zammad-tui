mod api;
mod app;
mod config;
mod kitty;
mod ui;

use anyhow::Result;
use app::{App, AppAction, BgResult};
use config::Config;
use crossterm::{
    event::{Event, EventStream, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use ratatui::prelude::*;
use std::io::Write;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;

    print!("Connecting to {}...", config.url);
    std::io::stdout().flush()?;

    let (tx, rx) = mpsc::unbounded_channel::<BgResult>();
    let mut app = App::new(config, tx).await?;
    println!(" ok");

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &mut app, rx).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(ref e) = result {
        eprintln!("Error: {e:#}");
    }

    result
}

async fn run_app<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    mut rx: mpsc::UnboundedReceiver<BgResult>,
) -> Result<()> {
    let mut event_stream = EventStream::new();

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Check if a background image download completed — needs TUI suspension
        if let Some((data, filename)) = app.pending_image.take() {
            suspend_for_image(terminal, &data, &filename)?;
            continue;
        }

        tokio::select! {
            // Terminal events (keyboard input)
            maybe_event = event_stream.next() => {
                let Some(Ok(event)) = maybe_event else {
                    break; // Stream ended
                };
                if let Event::Key(key) = event
                    && key.kind == KeyEventKind::Press {
                        let action = app.handle_key(key);
                        match action {
                            AppAction::None => {}
                            AppAction::Quit => break,
                            AppAction::OpenEditor {
                                ticket_id,
                                to,
                                cc,
                                subject,
                            } => {
                                let body = suspend_for_editor(terminal, &subject)?;
                                if let Some(body) = body {
                                    if !body.trim().is_empty() {
                                        app.spawn_send_reply(
                                            ticket_id,
                                            &to,
                                            cc.as_deref(),
                                            &subject,
                                            &body,
                                        );
                                    } else {
                                        app.message = Some((
                                            "Reply cancelled (empty body)".to_string(),
                                            app::MessageKind::Info,
                                        ));
                                    }
                                }
                            }
                            AppAction::OpenNoteEditor {
                                ticket_id,
                                subject,
                            } => {
                                let body = suspend_for_note_editor(terminal, &subject)?;
                                if let Some(body) = body {
                                    if !body.trim().is_empty() {
                                        app.spawn_send_note(ticket_id, &subject, &body);
                                    } else {
                                        app.message = Some((
                                            "Note cancelled (empty body)".to_string(),
                                            app::MessageKind::Info,
                                        ));
                                    }
                                }
                            }
                        }
                    }
            }
            // Background task results
            Some(result) = rx.recv() => {
                app.handle_bg_result(result);
            }
        }
    }
    Ok(())
}

fn suspend_for_editor<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    subject: &str,
) -> Result<Option<String>> {
    let mut tmpfile = tempfile::Builder::new().suffix(".md").tempfile()?;
    writeln!(
        tmpfile,
        "# Reply: {subject}\n# Lines starting with # will be removed.\n# Save and close the editor to send.\n"
    )?;
    let path = tmpfile.path().to_path_buf();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new(&editor).arg(&path).status()?;

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.hide_cursor()?;
    terminal.clear()?;

    if !status.success() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)?;
    let body: String = content
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    Ok(Some(body))
}

fn suspend_for_note_editor<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    subject: &str,
) -> Result<Option<String>> {
    let mut tmpfile = tempfile::Builder::new().suffix(".md").tempfile()?;
    writeln!(
        tmpfile,
        "# Internal note: {subject}\n# Lines starting with # will be removed.\n# Save and close the editor to add the note.\n"
    )?;
    let path = tmpfile.path().to_path_buf();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new(&editor).arg(&path).status()?;

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.hide_cursor()?;
    terminal.clear()?;

    if !status.success() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)?;
    let body: String = content
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    Ok(Some(body))
}

fn suspend_for_image<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    data: &[u8],
    filename: &str,
) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let result = kitty::show_image_fullscreen(data, filename);
    if let Err(e) = &result {
        eprintln!("Failed to display image: {e}");
        eprintln!("Press Enter to continue...");
        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf)?;
    }

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.hide_cursor()?;
    terminal.clear()?;

    Ok(())
}
