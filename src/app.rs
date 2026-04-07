use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;
use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::api::ZammadClient;
use crate::api::types::*;
use crate::config::Config;

#[derive(Debug, Clone, PartialEq)]
pub enum View {
    TicketList,
    TicketDetail,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortField {
    Updated,
    Created,
    Priority,
    State,
    Title,
    Assignee,
}

impl SortField {
    pub fn label(self) -> &'static str {
        match self {
            Self::Updated => "updated",
            Self::Created => "created",
            Self::Priority => "priority",
            Self::State => "state",
            Self::Title => "title",
            Self::Assignee => "assignee",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Updated => Self::Created,
            Self::Created => Self::Priority,
            Self::Priority => Self::State,
            Self::State => Self::Title,
            Self::Title => Self::Assignee,
            Self::Assignee => Self::Updated,
        }
    }
}

#[derive(Debug)]
pub enum Modal {
    StatusSelect {
        items: Vec<TicketState>,
        filtered: Vec<usize>,
        filter: String,
        selected: usize,
    },
    AssigneeSelect {
        items: Vec<User>,
        filtered: Vec<usize>,
        filter: String,
        selected: usize,
    },
    PrioritySelect {
        items: Vec<TicketPriority>,
        filtered: Vec<usize>,
        filter: String,
        selected: usize,
    },
    GroupSelect {
        items: Vec<(usize, String)>, // (tab_index, name)
        filtered: Vec<usize>,
        filter: String,
        selected: usize,
    },
    AttachmentSelect {
        /// (article_index, attachment_index, label)
        items: Vec<(usize, usize, String)>,
        selected: usize,
    },
    ArticleJump {
        /// (article_index, label)
        items: Vec<(usize, String)>,
        filtered: Vec<usize>,
        filter: String,
        selected: usize,
    },
    LinkSelect {
        items: Vec<String>,
        selected: usize,
    },
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageKind {
    Info,
    Error,
}

/// Actions that require terminal suspension (handled by main loop)
pub enum AppAction {
    None,
    Quit,
    OpenEditor {
        ticket_id: u64,
        to: String,
        cc: Option<String>,
        subject: String,
    },
    OpenNoteEditor {
        ticket_id: u64,
        subject: String,
    },
}

/// Results from background API calls
pub enum BgResult {
    TicketsLoaded {
        tab: usize,
        result: Result<Vec<Ticket>>,
    },
    TicketOpened(Result<(Ticket, Vec<TicketArticle>)>),
    TicketUpdated {
        result: Result<Ticket>,
        message: String,
    },
    ReplySent {
        result: Result<Vec<TicketArticle>>,
    },
    AttachmentDownloaded {
        filename: String,
        result: Result<Vec<u8>>,
    },
    ImageReady {
        filename: String,
        result: Result<Vec<u8>>,
    },
    UsersSearched {
        query: String,
        result: Result<Vec<User>>,
    },
    UserResolved {
        user_id: u64,
        result: Result<User>,
    },
}

pub struct App {
    pub client: ZammadClient,
    tx: mpsc::UnboundedSender<BgResult>,

    // Current user
    pub me: User,

    // Cached reference data
    pub groups: Vec<Group>,
    pub states: Vec<TicketState>,
    pub priorities: Vec<TicketPriority>,

    // View
    pub view: View,

    // Ticket list state
    pub tickets: Vec<Ticket>,
    pub ticket_list_state: TableState,
    pub current_tab: usize, // 0 = My Tickets, 1+ = group index
    pub tabs: Vec<String>,
    pub hide_closed: bool,
    pub sort_field: SortField,
    pub sort_ascending: bool,

    // Search
    pub search_mode: bool,
    pub search_query: String,

    // Ticket detail state
    pub current_ticket: Option<Ticket>,
    pub articles: Vec<TicketArticle>,
    pub detail_scroll: usize,
    pub selected_article: usize,

    // Modal
    pub modal: Option<Modal>,

    // Status message
    pub message: Option<(String, MessageKind)>,

    // Loading
    pub loading: bool,

    // User name cache (user_id -> display name)
    pub user_cache: HashMap<u64, String>,

    // Pending image data (set by BgResult::ImageReady, consumed by main loop)
    pub pending_image: Option<(Vec<u8>, String)>,
}

impl App {
    pub async fn new(config: Config, tx: mpsc::UnboundedSender<BgResult>) -> Result<Self> {
        let client = ZammadClient::new(&config)?;

        // Fetch initial data in parallel (blocking is OK before TUI starts)
        let (me, groups, states, priorities) = tokio::try_join!(
            client.get_me(),
            client.get_groups(),
            client.get_ticket_states(),
            client.get_ticket_priorities(),
        )?;

        let active_groups: Vec<Group> = groups
            .into_iter()
            .filter(|g| g.active.unwrap_or(true))
            .collect();

        let mut tabs = vec!["My Tickets".to_string()];
        for g in &active_groups {
            tabs.push(g.name.clone());
        }

        // Initial ticket fetch (blocking) — filter closed tickets server-side
        let closed_ids: Vec<u64> = states
            .iter()
            .filter(|s| s.is_closed())
            .map(|s| s.id)
            .collect();
        let exclusions: String = closed_ids
            .iter()
            .map(|id| format!(" NOT state_id:{id}"))
            .collect();
        let query = format!("owner_id:{}{exclusions}", me.id);
        let mut tickets = client.search_tickets(&query).await.unwrap_or_default();
        tickets.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        let mut ticket_list_state = TableState::default();
        if !tickets.is_empty() {
            ticket_list_state.select(Some(0));
        }

        let mut user_cache = HashMap::new();
        user_cache.insert(me.id, me.display_name());

        let app = Self {
            client,
            tx,
            me,
            groups: active_groups,
            states,
            priorities,
            view: View::TicketList,
            tickets,
            ticket_list_state,
            current_tab: 0,
            tabs,
            hide_closed: true,
            sort_field: SortField::Updated,
            sort_ascending: false,
            search_mode: false,
            search_query: String::new(),
            current_ticket: None,
            articles: Vec::new(),
            detail_scroll: 0,
            selected_article: 0,
            modal: None,
            message: None,
            loading: false,
            user_cache,
            pending_image: None,
        };
        app.resolve_unknown_users();
        Ok(app)
    }

    // -- Lookup helpers --

    pub fn state_name(&self, state_id: u64) -> &str {
        self.states
            .iter()
            .find(|s| s.id == state_id)
            .map(|s| s.name.as_str())
            .unwrap_or("?")
    }

    pub fn group_name(&self, group_id: u64) -> &str {
        self.groups
            .iter()
            .find(|g| g.id == group_id)
            .map(|g| g.name.as_str())
            .unwrap_or("?")
    }

    pub fn user_name(&self, user_id: u64) -> String {
        if let Some(name) = self.user_cache.get(&user_id) {
            return name.clone();
        }
        format!("#{user_id}")
    }

    pub fn priority_name(&self, priority_id: u64) -> &str {
        self.priorities
            .iter()
            .find(|p| p.id == priority_id)
            .map(|p| p.name.as_str())
            .unwrap_or("?")
    }

    fn closed_state_ids(&self) -> Vec<u64> {
        self.states
            .iter()
            .filter(|s| s.is_closed())
            .map(|s| s.id)
            .collect()
    }

    // -- User resolution --

    fn resolve_unknown_users(&self) {
        let unknown: Vec<u64> = self
            .tickets
            .iter()
            .map(|t| t.owner_id)
            .filter(|&id| id != 0 && !self.user_cache.contains_key(&id))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        for user_id in unknown {
            self.spawn_resolve_user(user_id);
        }
    }

    fn spawn_resolve_user(&self, user_id: u64) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client.get_user(user_id).await;
            let _ = tx.send(BgResult::UserResolved { user_id, result });
        });
    }

    // -- Background task spawning --

    fn spawn_refresh_tickets(&mut self) {
        self.loading = true;
        let client = self.client.clone();
        let tx = self.tx.clone();
        let tab = self.current_tab;
        let query = self.build_ticket_query();
        tokio::spawn(async move {
            let result = client.search_tickets(&query).await;
            let _ = tx.send(BgResult::TicketsLoaded { tab, result });
        });
    }

    fn spawn_open_ticket(&mut self, ticket_id: u64) {
        self.loading = true;
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = async {
                let (ticket, articles) = tokio::try_join!(
                    client.get_ticket(ticket_id),
                    client.get_articles(ticket_id),
                )?;
                Ok((ticket, articles))
            }
            .await;
            let _ = tx.send(BgResult::TicketOpened(result));
        });
    }

    fn spawn_update_ticket(&self, ticket_id: u64, update: UpdateTicket, message: String) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client.update_ticket(ticket_id, &update).await;
            let _ = tx.send(BgResult::TicketUpdated { result, message });
        });
    }

    pub fn spawn_send_reply(
        &self,
        ticket_id: u64,
        to: &str,
        cc: Option<&str>,
        subject: &str,
        body: &str,
    ) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        let article = CreateArticle {
            ticket_id,
            article_type: "email".to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
            content_type: "text/plain".to_string(),
            to: Some(to.to_string()),
            cc: cc.map(|s| s.to_string()),
            internal: false,
        };
        self.loading_msg("Sending reply...");
        tokio::spawn(async move {
            let result = async {
                client.create_article(&article).await?;
                client.get_articles(ticket_id).await
            }
            .await;
            let _ = tx.send(BgResult::ReplySent { result });
        });
    }

    fn spawn_download_attachment(
        &self,
        ticket_id: u64,
        article_id: u64,
        attachment_id: u64,
        filename: String,
    ) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        self.loading_msg("Downloading...");
        tokio::spawn(async move {
            let result = client
                .download_attachment(ticket_id, article_id, attachment_id)
                .await;
            let _ = tx.send(BgResult::AttachmentDownloaded { filename, result });
        });
    }

    fn spawn_view_image(
        &self,
        ticket_id: u64,
        article_id: u64,
        attachment_id: u64,
        filename: String,
    ) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        self.loading_msg("Loading image...");
        tokio::spawn(async move {
            let result = client
                .download_attachment(ticket_id, article_id, attachment_id)
                .await;
            let _ = tx.send(BgResult::ImageReady { filename, result });
        });
    }

    fn loading_msg(&self, _msg: &str) {
        // Message will show via self.loading flag in UI
    }

    fn build_ticket_query(&self) -> String {
        let base = if self.current_tab == 0 {
            format!("owner_id:{}", self.me.id)
        } else {
            let group = &self.groups[self.current_tab - 1];
            format!("group_id:{}", group.id)
        };
        if self.hide_closed {
            let exclusions: Vec<String> = self
                .closed_state_ids()
                .iter()
                .map(|id| format!("NOT state_id:{id}"))
                .collect();
            if exclusions.is_empty() {
                base
            } else {
                format!("{base} {}", exclusions.join(" "))
            }
        } else {
            base
        }
    }

    fn apply_filters(&mut self) {
        if self.hide_closed {
            let closed_ids = self.closed_state_ids();
            self.tickets.retain(|t| !closed_ids.contains(&t.state_id));
        }
        self.sort_tickets();
    }

    fn sort_tickets(&mut self) {
        let asc = self.sort_ascending;
        match self.sort_field {
            SortField::Updated => self.tickets.sort_by(|a, b| {
                let ord = a.updated_at.cmp(&b.updated_at);
                if asc { ord } else { ord.reverse() }
            }),
            SortField::Created => self.tickets.sort_by(|a, b| {
                let ord = a.created_at.cmp(&b.created_at);
                if asc { ord } else { ord.reverse() }
            }),
            SortField::Priority => self.tickets.sort_by(|a, b| {
                let ord = a.priority_id.cmp(&b.priority_id);
                if asc { ord } else { ord.reverse() }
            }),
            SortField::State => {
                let states = &self.states;
                self.tickets.sort_by(|a, b| {
                    let sa = states.iter().find(|s| s.id == a.state_id).map(|s| &s.name);
                    let sb = states.iter().find(|s| s.id == b.state_id).map(|s| &s.name);
                    let ord = sa.cmp(&sb);
                    if asc { ord } else { ord.reverse() }
                });
            }
            SortField::Title => self.tickets.sort_by(|a, b| {
                let ord = a.title.to_lowercase().cmp(&b.title.to_lowercase());
                if asc { ord } else { ord.reverse() }
            }),
            SortField::Assignee => {
                let cache = &self.user_cache;
                self.tickets.sort_by(|a, b| {
                    let na = cache.get(&a.owner_id).cloned().unwrap_or_default();
                    let nb = cache.get(&b.owner_id).cloned().unwrap_or_default();
                    let ord = na.to_lowercase().cmp(&nb.to_lowercase());
                    if asc { ord } else { ord.reverse() }
                });
            }
        }
    }

    // -- Handle background results --

    pub fn handle_bg_result(&mut self, result: BgResult) {
        match result {
            BgResult::TicketsLoaded { tab, result } => {
                self.loading = false;
                // Discard stale results from a different tab
                if tab != self.current_tab {
                    return;
                }
                match result {
                    Ok(tickets) => {
                        self.tickets = tickets;
                        self.apply_filters();
                        self.resolve_unknown_users();
                        if self.tickets.is_empty() {
                            self.ticket_list_state.select(None);
                        } else {
                            self.ticket_list_state.select(Some(0));
                        }
                    }
                    Err(e) => {
                        self.message = Some((format!("Error: {e}"), MessageKind::Error));
                    }
                }
            }
            BgResult::TicketOpened(result) => {
                self.loading = false;
                match result {
                    Ok((ticket, mut articles)) => {
                        articles.reverse(); // newest first
                        self.current_ticket = Some(ticket);
                        self.articles = articles;
                        self.detail_scroll = 0;
                        self.selected_article = 0;
                        self.view = View::TicketDetail;
                    }
                    Err(e) => {
                        self.message = Some((format!("Error: {e}"), MessageKind::Error));
                    }
                }
            }
            BgResult::TicketUpdated { result, message } => match result {
                Ok(updated) => {
                    self.message = Some((message, MessageKind::Info));
                    self.current_ticket = Some(updated);
                }
                Err(e) => {
                    self.message = Some((format!("Error: {e}"), MessageKind::Error));
                }
            },
            BgResult::ReplySent { result } => {
                self.loading = false;
                match result {
                    Ok(mut articles) => {
                        articles.reverse(); // newest first
                        self.articles = articles;
                        self.selected_article = 0;
                        self.detail_scroll = 0;
                        self.message = Some(("Reply sent".to_string(), MessageKind::Info));
                    }
                    Err(e) => {
                        self.message = Some((format!("Send failed: {e}"), MessageKind::Error));
                    }
                }
            }
            BgResult::AttachmentDownloaded { filename, result } => match result {
                Ok(data) => {
                    let path = std::env::current_dir().unwrap_or_default().join(&filename);
                    match std::fs::write(&path, &data) {
                        Ok(()) => {
                            self.message =
                                Some((format!("Saved: {}", path.display()), MessageKind::Info));
                        }
                        Err(e) => {
                            self.message = Some((format!("Write failed: {e}"), MessageKind::Error));
                        }
                    }
                }
                Err(e) => {
                    self.message = Some((format!("Download failed: {e}"), MessageKind::Error));
                }
            },
            BgResult::ImageReady { filename, result } => match result {
                Ok(data) => {
                    self.pending_image = Some((data, filename));
                }
                Err(e) => {
                    self.message = Some((format!("Download failed: {e}"), MessageKind::Error));
                }
            },
            BgResult::UsersSearched { query, result } => {
                // Only update if the assignee modal is still open and filter matches
                if let Some(Modal::AssigneeSelect {
                    items,
                    filtered,
                    filter,
                    selected,
                }) = &mut self.modal
                {
                    // Discard stale results: "*" is the initial wildcard, otherwise must match current filter
                    if query == "*" && filter.is_empty() || query == *filter {
                        match result {
                            Ok(users) => {
                                *items = users
                                    .into_iter()
                                    .filter(|u| u.active.unwrap_or(true))
                                    .collect();
                                *filtered = (0..items.len()).collect();
                                *selected = 0;
                            }
                            Err(e) => {
                                self.message =
                                    Some((format!("User search failed: {e}"), MessageKind::Error));
                            }
                        }
                    }
                }
            }
            BgResult::UserResolved { user_id, result } => {
                if let Ok(user) = result {
                    self.user_cache.insert(user_id, user.display_name());
                }
            }
        }
    }

    // -- Navigation --

    pub fn selected_ticket(&self) -> Option<&Ticket> {
        self.ticket_list_state
            .selected()
            .and_then(|i| self.tickets.get(i))
    }

    pub fn filtered_tickets(&self) -> &[Ticket] {
        &self.tickets
    }

    // -- Key handling (all synchronous) --

    pub fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        self.message = None;

        if self.modal.is_some() {
            return self.handle_modal_key(key);
        }

        if self.search_mode {
            return self.handle_search_key(key);
        }

        match self.view {
            View::TicketList => self.handle_ticket_list_key(key),
            View::TicketDetail => self.handle_ticket_detail_key(key),
        }
    }

    fn handle_ticket_list_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return AppAction::Quit,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return AppAction::Quit;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let len = self.tickets.len();
                if len > 0 {
                    let i = self.ticket_list_state.selected().unwrap_or(0);
                    self.ticket_list_state.select(Some((i + 1).min(len - 1)));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let i = self.ticket_list_state.selected().unwrap_or(0);
                self.ticket_list_state.select(Some(i.saturating_sub(1)));
            }
            KeyCode::PageDown => {
                let len = self.tickets.len();
                if len > 0 {
                    let i = self.ticket_list_state.selected().unwrap_or(0);
                    self.ticket_list_state.select(Some((i + 20).min(len - 1)));
                }
            }
            KeyCode::PageUp => {
                let i = self.ticket_list_state.selected().unwrap_or(0);
                self.ticket_list_state.select(Some(i.saturating_sub(20)));
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.ticket_list_state.select(Some(0));
            }
            KeyCode::Char('G') | KeyCode::End => {
                let len = self.tickets.len();
                if len > 0 {
                    self.ticket_list_state.select(Some(len - 1));
                }
            }
            KeyCode::Enter => {
                if let Some(ticket) = self.selected_ticket().cloned() {
                    self.spawn_open_ticket(ticket.id);
                }
            }
            KeyCode::Tab => {
                self.current_tab = (self.current_tab + 1) % self.tabs.len();
                self.spawn_refresh_tickets();
            }
            KeyCode::BackTab => {
                if self.current_tab == 0 {
                    self.current_tab = self.tabs.len() - 1;
                } else {
                    self.current_tab -= 1;
                }
                self.spawn_refresh_tickets();
            }
            KeyCode::Char('f') => {
                self.hide_closed = !self.hide_closed;
                self.spawn_refresh_tickets();
                let status = if self.hide_closed {
                    "Hiding closed tickets"
                } else {
                    "Showing all tickets"
                };
                self.message = Some((status.to_string(), MessageKind::Info));
            }
            KeyCode::Char('o') => {
                self.sort_field = self.sort_field.next();
                self.sort_tickets();
                let dir = if self.sort_ascending { "asc" } else { "desc" };
                self.message = Some((
                    format!("Sort: {} ({})", self.sort_field.label(), dir),
                    MessageKind::Info,
                ));
            }
            KeyCode::Char('O') => {
                self.sort_ascending = !self.sort_ascending;
                self.sort_tickets();
                let dir = if self.sort_ascending { "asc" } else { "desc" };
                self.message = Some((
                    format!("Sort: {} ({})", self.sort_field.label(), dir),
                    MessageKind::Info,
                ));
            }
            KeyCode::Char('t') => {
                self.open_group_modal();
            }
            KeyCode::Char('/') => {
                self.search_mode = true;
                self.search_query.clear();
            }
            KeyCode::Char('R') => {
                self.spawn_refresh_tickets();
            }
            KeyCode::Char('?') => {
                self.modal = Some(Modal::Help);
            }
            _ => {}
        }
        AppAction::None
    }

    fn handle_ticket_detail_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.view = View::TicketList;
                self.current_ticket = None;
                self.articles.clear();
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return AppAction::Quit;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.detail_scroll = self.detail_scroll.saturating_add(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.detail_scroll = self.detail_scroll.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.detail_scroll = self.detail_scroll.saturating_add(20);
            }
            KeyCode::PageUp => {
                self.detail_scroll = self.detail_scroll.saturating_sub(20);
            }
            KeyCode::Home => {
                self.detail_scroll = 0;
            }
            KeyCode::End => {
                self.detail_scroll = usize::MAX / 2;
            }
            KeyCode::Char('n') => {
                if !self.articles.is_empty() {
                    self.selected_article =
                        (self.selected_article + 1).min(self.articles.len() - 1);
                    // Each collapsed article is ~2 lines; scroll so selected is near top
                    self.detail_scroll = self.selected_article * 2;
                }
            }
            KeyCode::Char('p') => {
                self.selected_article = self.selected_article.saturating_sub(1);
                self.detail_scroll = self.selected_article * 2;
            }
            KeyCode::Char(c @ '1'..='9') => {
                let idx = (c as usize) - ('1' as usize);
                if idx < self.articles.len() {
                    self.selected_article = idx;
                    self.detail_scroll = idx * 2;
                }
            }
            KeyCode::Char('r') => {
                return self.prepare_reply();
            }
            KeyCode::Char('N') => {
                return self.prepare_note();
            }
            KeyCode::Char('s') => {
                self.open_status_modal();
            }
            KeyCode::Char('a') => {
                self.open_assignee_modal();
            }
            KeyCode::Char('P') => {
                self.open_priority_modal();
            }
            KeyCode::Char('d') | KeyCode::Char('v') | KeyCode::Char('i') => {
                self.open_attachment_modal();
            }
            KeyCode::Char('#') | KeyCode::Char('J') => {
                self.open_article_jump_modal();
            }
            KeyCode::Char('o') => {
                self.open_link_modal();
            }
            KeyCode::Char('R') => {
                if let Some(ticket) = &self.current_ticket {
                    self.spawn_open_ticket(ticket.id);
                }
            }
            KeyCode::Char('?') => {
                self.modal = Some(Modal::Help);
            }
            _ => {}
        }
        AppAction::None
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Enter => {
                self.search_mode = false;
                if !self.search_query.is_empty() {
                    let query = self.search_query.clone();
                    let client = self.client.clone();
                    let tx = self.tx.clone();
                    let tab = self.current_tab;
                    self.loading = true;
                    tokio::spawn(async move {
                        let result = client.search_tickets(&query).await;
                        let _ = tx.send(BgResult::TicketsLoaded { tab, result });
                    });
                }
            }
            KeyCode::Esc => {
                self.search_mode = false;
                self.search_query.clear();
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
            }
            KeyCode::Backspace => {
                self.search_query.pop();
            }
            _ => {}
        }
        AppAction::None
    }

    fn handle_modal_key(&mut self, key: KeyEvent) -> AppAction {
        let mut modal = self.modal.take().unwrap();

        match key.code {
            KeyCode::Esc => {
                // Close modal
                return AppAction::None;
            }
            KeyCode::Enter => {
                self.confirm_modal_selection(&modal);
                return AppAction::None;
            }
            KeyCode::Down | KeyCode::Char('j') if !matches!(modal, Modal::Help) => {
                match &mut modal {
                    Modal::StatusSelect {
                        filtered, selected, ..
                    }
                    | Modal::PrioritySelect {
                        filtered, selected, ..
                    }
                    | Modal::AssigneeSelect {
                        filtered, selected, ..
                    }
                    | Modal::GroupSelect {
                        filtered, selected, ..
                    }
                    | Modal::ArticleJump {
                        filtered, selected, ..
                    } => {
                        if !filtered.is_empty() {
                            *selected = (*selected + 1).min(filtered.len() - 1);
                        }
                    }
                    Modal::AttachmentSelect { items, selected } => {
                        if !items.is_empty() {
                            *selected = (*selected + 1).min(items.len() - 1);
                        }
                    }
                    Modal::LinkSelect { items, selected } => {
                        if !items.is_empty() {
                            *selected = (*selected + 1).min(items.len() - 1);
                        }
                    }
                    Modal::Help => {}
                }
                self.modal = Some(modal);
            }
            KeyCode::Up | KeyCode::Char('k') if !matches!(modal, Modal::Help) => {
                match &mut modal {
                    Modal::StatusSelect { selected, .. }
                    | Modal::AssigneeSelect { selected, .. }
                    | Modal::PrioritySelect { selected, .. }
                    | Modal::GroupSelect { selected, .. }
                    | Modal::ArticleJump { selected, .. }
                    | Modal::AttachmentSelect { selected, .. }
                    | Modal::LinkSelect { selected, .. } => {
                        *selected = selected.saturating_sub(1);
                    }
                    Modal::Help => {}
                }
                self.modal = Some(modal);
            }
            KeyCode::Char(c) => {
                let is_assignee = matches!(modal, Modal::AssigneeSelect { .. });
                update_modal_filter(&mut modal, |f| f.push(c));
                self.modal = Some(modal);
                if is_assignee {
                    self.trigger_assignee_search();
                }
            }
            KeyCode::Backspace => {
                let is_assignee = matches!(modal, Modal::AssigneeSelect { .. });
                update_modal_filter(&mut modal, |f| {
                    f.pop();
                });
                self.modal = Some(modal);
                if is_assignee {
                    self.trigger_assignee_search();
                }
            }
            _ => {
                self.modal = Some(modal);
            }
        }
        AppAction::None
    }

    fn confirm_modal_selection(&mut self, modal: &Modal) {
        match modal {
            Modal::GroupSelect {
                items,
                filtered,
                selected,
                ..
            } => {
                if let Some(&idx) = filtered.get(*selected) {
                    let (tab_index, _) = &items[idx];
                    self.current_tab = *tab_index;
                    self.spawn_refresh_tickets();
                }
                return;
            }
            Modal::AttachmentSelect { items, selected } => {
                if let Some((article_idx, att_idx, _)) = items.get(*selected) {
                    self.confirm_attachment_download(*article_idx, *att_idx);
                }
                return;
            }
            Modal::ArticleJump {
                items,
                filtered,
                selected,
                ..
            } => {
                if let Some(&idx) = filtered.get(*selected) {
                    let (article_idx, _) = &items[idx];
                    self.selected_article = *article_idx;
                    self.detail_scroll = *article_idx * 2;
                }
                return;
            }
            Modal::LinkSelect { items, selected } => {
                if let Some(url) = items.get(*selected) {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(url)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn();
                    self.message = Some((format!("Opened: {url}"), MessageKind::Info));
                }
                return;
            }
            Modal::Help => return,
            _ => {}
        }

        let ticket_id = match &self.current_ticket {
            Some(t) => t.id,
            None => return,
        };

        match modal {
            Modal::StatusSelect {
                items,
                filtered,
                selected,
                ..
            } => {
                if let Some(&idx) = filtered.get(*selected) {
                    let state = &items[idx];
                    let update = UpdateTicket {
                        state_id: Some(state.id),
                        ..Default::default()
                    };
                    self.spawn_update_ticket(
                        ticket_id,
                        update,
                        format!("Status changed to '{}'", state.name),
                    );
                }
            }
            Modal::AssigneeSelect {
                items,
                filtered,
                selected,
                ..
            } => {
                if let Some(&idx) = filtered.get(*selected) {
                    let user = &items[idx];
                    let update = UpdateTicket {
                        owner_id: Some(user.id),
                        ..Default::default()
                    };
                    self.spawn_update_ticket(
                        ticket_id,
                        update,
                        format!("Assigned to '{}'", user.display_name()),
                    );
                }
            }
            Modal::PrioritySelect {
                items,
                filtered,
                selected,
                ..
            } => {
                if let Some(&idx) = filtered.get(*selected) {
                    let priority = &items[idx];
                    let update = UpdateTicket {
                        priority_id: Some(priority.id),
                        ..Default::default()
                    };
                    self.spawn_update_ticket(
                        ticket_id,
                        update,
                        format!("Priority changed to '{}'", priority.name),
                    );
                }
            }
            Modal::GroupSelect { .. }
            | Modal::AttachmentSelect { .. }
            | Modal::ArticleJump { .. }
            | Modal::LinkSelect { .. }
            | Modal::Help => {
                unreachable!()
            }
        }
    }

    // -- Reply --

    fn prepare_reply(&self) -> AppAction {
        let ticket = match &self.current_ticket {
            Some(t) => t,
            None => return AppAction::None,
        };

        let my_email = self.me.email.as_deref().unwrap_or("");
        let mut to_addrs: Vec<String> = Vec::new();
        let mut cc_addrs: Vec<String> = Vec::new();

        for article in &self.articles {
            if let Some(from) = &article.from {
                for addr in parse_email_list(from) {
                    if !addr.eq_ignore_ascii_case(my_email)
                        && !to_addrs.iter().any(|a| a.eq_ignore_ascii_case(&addr))
                    {
                        to_addrs.push(addr);
                    }
                }
            }
            if let Some(to) = &article.to {
                for addr in parse_email_list(to) {
                    if !addr.eq_ignore_ascii_case(my_email)
                        && !to_addrs.iter().any(|a| a.eq_ignore_ascii_case(&addr))
                    {
                        to_addrs.push(addr);
                    }
                }
            }
            if let Some(cc) = &article.cc {
                for addr in parse_email_list(cc) {
                    if !addr.eq_ignore_ascii_case(my_email)
                        && !to_addrs.iter().any(|a| a.eq_ignore_ascii_case(&addr))
                        && !cc_addrs.iter().any(|a| a.eq_ignore_ascii_case(&addr))
                    {
                        cc_addrs.push(addr);
                    }
                }
            }
        }

        let subject = format!("Re: {}", ticket.title);
        let to = to_addrs.join(", ");
        let cc = if cc_addrs.is_empty() {
            None
        } else {
            Some(cc_addrs.join(", "))
        };

        AppAction::OpenEditor {
            ticket_id: ticket.id,
            to,
            cc,
            subject,
        }
    }

    // -- Internal note --

    fn prepare_note(&self) -> AppAction {
        let ticket = match &self.current_ticket {
            Some(t) => t,
            None => return AppAction::None,
        };
        AppAction::OpenNoteEditor {
            ticket_id: ticket.id,
            subject: ticket.title.clone(),
        }
    }

    pub fn spawn_send_note(&self, ticket_id: u64, subject: &str, body: &str) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        let article = CreateArticle {
            ticket_id,
            article_type: "note".to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
            content_type: "text/plain".to_string(),
            to: None,
            cc: None,
            internal: true,
        };
        tokio::spawn(async move {
            let result = async {
                client.create_article(&article).await?;
                client.get_articles(ticket_id).await
            }
            .await;
            let _ = tx.send(BgResult::ReplySent { result });
        });
    }

    // -- Modals --

    fn open_status_modal(&mut self) {
        let items: Vec<TicketState> = self
            .states
            .iter()
            .filter(|s| s.active.unwrap_or(true))
            .cloned()
            .collect();
        let filtered: Vec<usize> = (0..items.len()).collect();
        self.modal = Some(Modal::StatusSelect {
            items,
            filtered,
            filter: String::new(),
            selected: 0,
        });
    }

    fn open_assignee_modal(&mut self) {
        // Start with empty list; users are searched as you type
        self.modal = Some(Modal::AssigneeSelect {
            items: Vec::new(),
            filtered: Vec::new(),
            filter: String::new(),
            selected: 0,
        });
        // Pre-populate with a wildcard search
        self.spawn_user_search("*".to_string());
    }

    fn trigger_assignee_search(&self) {
        if let Some(Modal::AssigneeSelect { filter, .. }) = &self.modal {
            let query = if filter.is_empty() {
                "*".to_string()
            } else {
                filter.clone()
            };
            self.spawn_user_search(query);
        }
    }

    fn spawn_user_search(&self, query: String) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        let q = query.clone();
        tokio::spawn(async move {
            let result = client.search_users(&q).await;
            let _ = tx.send(BgResult::UsersSearched { query, result });
        });
    }

    fn open_priority_modal(&mut self) {
        let items: Vec<TicketPriority> = self
            .priorities
            .iter()
            .filter(|p| p.active.unwrap_or(true))
            .cloned()
            .collect();
        let filtered: Vec<usize> = (0..items.len()).collect();
        self.modal = Some(Modal::PrioritySelect {
            items,
            filtered,
            filter: String::new(),
            selected: 0,
        });
    }

    fn open_group_modal(&mut self) {
        let items: Vec<(usize, String)> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, name)| (i, name.clone()))
            .collect();
        let filtered: Vec<usize> = (0..items.len()).collect();
        self.modal = Some(Modal::GroupSelect {
            items,
            filtered,
            filter: String::new(),
            selected: self.current_tab,
        });
    }

    // -- Article jump --

    fn open_article_jump_modal(&mut self) {
        if self.articles.is_empty() {
            return;
        }
        let items: Vec<(usize, String)> = self
            .articles
            .iter()
            .enumerate()
            .map(|(i, article)| {
                let from = article.from.as_deref().unwrap_or("?");
                let date = self.format_time(&article.created_at);
                let article_type = article.article_type.as_deref().unwrap_or("note");
                let label = format!("#{} {} ({}) {}", i + 1, from, article_type, date);
                (i, label)
            })
            .collect();
        let filtered: Vec<usize> = (0..items.len()).collect();
        let selected = self.selected_article;
        self.modal = Some(Modal::ArticleJump {
            items,
            filtered,
            filter: String::new(),
            selected,
        });
    }

    // -- Attachments --

    // -- Links --

    fn open_link_modal(&mut self) {
        let article = match self.articles.get(self.selected_article) {
            Some(a) => a,
            None => return,
        };

        // Extract URLs from the raw body (works for both HTML href and plain text)
        let urls = extract_urls(&article.body);

        // Also include the Zammad ticket URL itself
        let mut items: Vec<String> = Vec::new();
        if let Some(ticket) = &self.current_ticket {
            let base = self.client.base_url();
            items.push(format!("{}/#ticket/zoom/{}", base, ticket.id));
        }

        for url in urls {
            if !items.contains(&url) {
                items.push(url);
            }
        }

        if items.is_empty() {
            self.message = Some(("No links found".to_string(), MessageKind::Info));
            return;
        }
        self.modal = Some(Modal::LinkSelect { items, selected: 0 });
    }

    // -- Attachments --

    fn open_attachment_modal(&mut self) {
        let mut items = Vec::new();
        for (ai, article) in self.articles.iter().enumerate() {
            for (ati, att) in article.attachments.iter().enumerate() {
                let icon = if att.is_image() { "[img] " } else { "" };
                let size = att
                    .size
                    .as_ref()
                    .and_then(|v| v.as_u64())
                    .map(|b| format_bytes(b))
                    .unwrap_or_default();
                let label = format!(
                    "Art.{}: {icon}{} ({}) {size}",
                    ai + 1,
                    att.filename,
                    att.mime_type(),
                );
                items.push((ai, ati, label));
            }
        }
        if items.is_empty() {
            self.message = Some(("No attachments".to_string(), MessageKind::Info));
            return;
        }
        // Pre-select first attachment of current article if possible
        let selected = items
            .iter()
            .position(|(ai, _, _)| *ai == self.selected_article)
            .unwrap_or(0);
        self.modal = Some(Modal::AttachmentSelect { items, selected });
    }

    fn confirm_attachment_download(&mut self, article_idx: usize, att_idx: usize) {
        let ticket = match &self.current_ticket {
            Some(t) => t,
            None => return,
        };
        let article = match self.articles.get(article_idx) {
            Some(a) => a,
            None => return,
        };
        let att = match article.attachments.get(att_idx) {
            Some(a) => a,
            None => return,
        };
        if att.is_image() {
            self.spawn_view_image(ticket.id, article.id, att.id, att.filename.clone());
        } else {
            self.spawn_download_attachment(ticket.id, article.id, att.id, att.filename.clone());
        }
    }

    // -- Helpers --

    pub fn format_time(&self, timestamp: &str) -> String {
        if let Some(t_pos) = timestamp.find('T') {
            let date = &timestamp[..t_pos];
            let time = &timestamp[t_pos + 1..];
            let time = time.split('.').next().unwrap_or(time);
            let time = time.trim_end_matches('Z');
            let time = &time[..5.min(time.len())];
            format!("{date} {time}")
        } else {
            timestamp.to_string()
        }
    }
}

/// Update the filter string and recompute filtered indices for any modal variant.
fn update_modal_filter(modal: &mut Modal, f: impl FnOnce(&mut String)) {
    match modal {
        Modal::StatusSelect {
            items,
            filtered,
            filter,
            selected,
        } => {
            f(filter);
            *filtered = filter_indices(items.iter().map(|s| s.name.as_str()), filter);
            *selected = 0;
        }
        Modal::AssigneeSelect {
            items,
            filtered,
            filter,
            selected,
        } => {
            f(filter);
            *filtered = filter_indices(
                items.iter().map(|u| u.display_name()).map(LeakedStr::from),
                filter,
            );
            *selected = 0;
        }
        Modal::PrioritySelect {
            items,
            filtered,
            filter,
            selected,
        } => {
            f(filter);
            *filtered = filter_indices(items.iter().map(|p| p.name.as_str()), filter);
            *selected = 0;
        }
        Modal::GroupSelect {
            items,
            filtered,
            filter,
            selected,
        } => {
            f(filter);
            *filtered = filter_indices(items.iter().map(|(_, name)| name.as_str()), filter);
            *selected = 0;
        }
        Modal::ArticleJump {
            items,
            filtered,
            filter,
            selected,
        } => {
            f(filter);
            *filtered = filter_indices(items.iter().map(|(_, label)| label.as_str()), filter);
            *selected = 0;
        }
        Modal::AttachmentSelect { .. } | Modal::LinkSelect { .. } => {
            // No filter for these modals
        }
        Modal::Help => {}
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn parse_email_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|part| {
            let part = part.trim();
            if let Some(start) = part.find('<')
                && let Some(end) = part.find('>')
            {
                return part[start + 1..end].trim().to_string();
            }
            part.to_string()
        })
        .filter(|s| s.contains('@'))
        .collect()
}

fn filter_indices(items: impl Iterator<Item = impl AsRef<str>>, filter: &str) -> Vec<usize> {
    let filter_lower = filter.to_lowercase();
    items
        .enumerate()
        .filter(|(_, name)| name.as_ref().to_lowercase().contains(&filter_lower))
        .map(|(i, _)| i)
        .collect()
}

struct LeakedStr(String);

impl From<String> for LeakedStr {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl AsRef<str> for LeakedStr {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Extract URLs (http/https) from text, handling both plain text and HTML href attributes.
fn extract_urls(text: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Match href="..." in HTML
    let mut rest = text;
    while let Some(pos) = rest.find("href=\"") {
        let start = pos + 6;
        rest = &rest[start..];
        if let Some(end) = rest.find('"') {
            let url = &rest[..end];
            if (url.starts_with("http://") || url.starts_with("https://")) && seen.insert(url.to_string()) {
                urls.push(url.to_string());
            }
            rest = &rest[end..];
        }
    }

    // Match bare URLs in text
    for prefix in ["https://", "http://"] {
        let mut rest = text;
        while let Some(pos) = rest.find(prefix) {
            rest = &rest[pos..];
            let end = rest
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '>' || c == '<' || c == ')' || c == ']')
                .unwrap_or(rest.len());
            let url = rest[..end].trim_end_matches(|c: char| c == '.' || c == ',' || c == ';');
            if seen.insert(url.to_string()) {
                urls.push(url.to_string());
            }
            rest = &rest[end.min(rest.len())..];
        }
    }

    urls
}
