pub mod types;

use anyhow::{Context, Result, bail};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use types::*;

use crate::config::Config;

#[derive(Clone)]
pub struct ZammadClient {
    client: reqwest::Client,
    base_url: String,
    log: Arc<Mutex<std::fs::File>>,
}

impl ZammadClient {
    pub fn new(config: &Config) -> Result<Self> {
        let mut headers = HeaderMap::new();
        let auth_value = format!("Token token={}", config.token);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value).context("Invalid token")?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let log_path = log_path()?;
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("Failed to open log: {}", log_path.display()))?;
        writeln!(
            log_file,
            "\n=== zui started at {} ===\n  base_url: {}",
            now_str(),
            config.url
        )?;
        eprintln!("Request log: {}", log_path.display());

        Ok(Self {
            client,
            base_url: config.url.clone(),
            log: Arc::new(Mutex::new(log_file)),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{}", self.base_url, path)
    }

    fn log(&self, msg: &str) {
        if let Ok(mut f) = self.log.lock() {
            let _ = writeln!(f, "[{}] {}", now_str(), msg);
        }
    }

    /// Perform a GET request, log timing, return deserialized JSON.
    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        self.log(&format!("GET {url} ..."));
        let start = Instant::now();

        let resp = match self.client.get(url).send().await {
            Ok(r) => r,
            Err(e) => {
                let elapsed = start.elapsed();
                self.log(&format!("GET {url} -> SEND ERROR after {elapsed:.2?}: {e}"));
                return Err(e.into());
            }
        };

        let status = resp.status();
        let content_len = resp.content_length();
        let elapsed_send = start.elapsed();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let elapsed = start.elapsed();
            self.log(&format!(
                "GET {url} -> {status} after {elapsed:.2?} (send: {elapsed_send:.2?})\n  body: {}",
                truncate_log(&body, 500)
            ));
            bail!("GET {url}: {status}\n{body}");
        }

        let body_bytes = resp.bytes().await?;
        let body_len = body_bytes.len();
        let elapsed = start.elapsed();

        self.log(&format!(
            "GET {url} -> {status}, {body_len} bytes in {elapsed:.2?} (send: {elapsed_send:.2?}, content-length: {content_len:?})"
        ));

        serde_json::from_slice(&body_bytes).with_context(|| {
            let preview = String::from_utf8_lossy(&body_bytes[..body_bytes.len().min(500)]);
            self.log(&format!(
                "GET {url} -> JSON PARSE ERROR\n  body preview: {preview}"
            ));
            format!("GET {url}: failed to parse JSON response")
        })
    }

    /// Perform a PUT request with JSON body, log timing.
    async fn put_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        let req_body = serde_json::to_string(body).unwrap_or_default();
        self.log(&format!(
            "PUT {url} ...\n  body: {}",
            truncate_log(&req_body, 300)
        ));
        let start = Instant::now();

        let resp = match self.client.put(url).json(body).send().await {
            Ok(r) => r,
            Err(e) => {
                let elapsed = start.elapsed();
                self.log(&format!("PUT {url} -> SEND ERROR after {elapsed:.2?}: {e}"));
                return Err(e.into());
            }
        };

        let status = resp.status();
        let elapsed_send = start.elapsed();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let elapsed = start.elapsed();
            self.log(&format!(
                "PUT {url} -> {status} after {elapsed:.2?}\n  body: {}",
                truncate_log(&body, 500)
            ));
            bail!("PUT {url}: {status}\n{body}");
        }

        let body_bytes = resp.bytes().await?;
        let elapsed = start.elapsed();
        self.log(&format!(
            "PUT {url} -> {status}, {} bytes in {elapsed:.2?} (send: {elapsed_send:.2?})",
            body_bytes.len()
        ));

        serde_json::from_slice(&body_bytes).with_context(|| {
            let preview = String::from_utf8_lossy(&body_bytes[..body_bytes.len().min(500)]);
            self.log(&format!(
                "PUT {url} -> JSON PARSE ERROR\n  body preview: {preview}"
            ));
            format!("PUT {url}: failed to parse JSON response")
        })
    }

    /// Perform a POST request with JSON body, log timing.
    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        let req_body = serde_json::to_string(body).unwrap_or_default();
        self.log(&format!(
            "POST {url} ...\n  body: {}",
            truncate_log(&req_body, 300)
        ));
        let start = Instant::now();

        let resp = match self.client.post(url).json(body).send().await {
            Ok(r) => r,
            Err(e) => {
                let elapsed = start.elapsed();
                self.log(&format!(
                    "POST {url} -> SEND ERROR after {elapsed:.2?}: {e}"
                ));
                return Err(e.into());
            }
        };

        let status = resp.status();
        let elapsed_send = start.elapsed();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let elapsed = start.elapsed();
            self.log(&format!(
                "POST {url} -> {status} after {elapsed:.2?}\n  body: {}",
                truncate_log(&body, 500)
            ));
            bail!("POST {url}: {status}\n{body}");
        }

        let body_bytes = resp.bytes().await?;
        let elapsed = start.elapsed();
        self.log(&format!(
            "POST {url} -> {status}, {} bytes in {elapsed:.2?} (send: {elapsed_send:.2?})",
            body_bytes.len()
        ));

        serde_json::from_slice(&body_bytes).with_context(|| {
            let preview = String::from_utf8_lossy(&body_bytes[..body_bytes.len().min(500)]);
            self.log(&format!(
                "POST {url} -> JSON PARSE ERROR\n  body preview: {preview}"
            ));
            format!("POST {url}: failed to parse JSON response")
        })
    }

    // -- Users --

    pub async fn get_me(&self) -> Result<User> {
        self.get_json(&self.url("/users/me")).await
    }

    pub async fn get_user(&self, id: u64) -> Result<User> {
        self.get_json(&self.url(&format!("/users/{id}"))).await
    }

    pub async fn search_users(&self, query: &str) -> Result<Vec<User>> {
        let encoded = urlencoded(query);
        let url = self.url(&format!(
            "/users/search?query={encoded}&per_page=50"
        ));
        self.get_json(&url).await
    }

    // -- Groups --

    pub async fn get_groups(&self) -> Result<Vec<Group>> {
        self.get_json(&self.url("/groups?per_page=100")).await
    }

    // -- Ticket States --

    pub async fn get_ticket_states(&self) -> Result<Vec<TicketState>> {
        self.get_json(&self.url("/ticket_states?per_page=100"))
            .await
    }

    // -- Ticket Priorities --

    pub async fn get_ticket_priorities(&self) -> Result<Vec<TicketPriority>> {
        self.get_json(&self.url("/ticket_priorities?per_page=100"))
            .await
    }

    // -- Tickets --

    pub async fn search_tickets(&self, query: &str) -> Result<Vec<Ticket>> {
        let mut all = Vec::new();
        let mut page = 1u32;
        let encoded_query = urlencoded(query);
        loop {
            let url = self.url(&format!(
                "/tickets/search?query={encoded_query}&page={page}&per_page=100"
            ));
            let resp: Vec<Ticket> = self.get_json(&url).await?;
            let done = resp.len() < 100;
            self.log(&format!(
                "  search page {page}: got {} tickets (done={done})",
                resp.len()
            ));
            all.extend(resp);
            if done {
                break;
            }
            page += 1;
        }
        self.log(&format!(
            "  search total: {} tickets for query={query}",
            all.len()
        ));
        Ok(all)
    }

    pub async fn get_ticket(&self, id: u64) -> Result<Ticket> {
        self.get_json(&self.url(&format!("/tickets/{id}"))).await
    }

    pub async fn update_ticket(&self, id: u64, update: &UpdateTicket) -> Result<Ticket> {
        self.put_json(&self.url(&format!("/tickets/{id}")), update)
            .await
    }

    // -- Articles --

    pub async fn get_articles(&self, ticket_id: u64) -> Result<Vec<TicketArticle>> {
        self.get_json(&self.url(&format!("/ticket_articles/by_ticket/{ticket_id}")))
            .await
    }

    pub async fn create_article(&self, article: &CreateArticle) -> Result<TicketArticle> {
        self.post_json(&self.url("/ticket_articles"), article).await
    }

    // -- Attachments --

    pub async fn download_attachment(
        &self,
        ticket_id: u64,
        article_id: u64,
        attachment_id: u64,
    ) -> Result<Vec<u8>> {
        let url = self.url(&format!(
            "/ticket_attachment/{ticket_id}/{article_id}/{attachment_id}"
        ));
        self.log(&format!("GET {url} (binary) ..."));
        let start = Instant::now();

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;

        let status = resp.status();
        let elapsed_send = start.elapsed();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            self.log(&format!(
                "GET {url} -> {status} after {elapsed_send:.2?}\n  body: {}",
                truncate_log(&body, 500)
            ));
            bail!("GET {url}: {status}");
        }

        let data = resp.bytes().await?.to_vec();
        let elapsed = start.elapsed();
        self.log(&format!(
            "GET {url} -> {status}, {} bytes in {elapsed:.2?} (send: {elapsed_send:.2?})",
            data.len()
        ));
        Ok(data)
    }
}

fn log_path() -> Result<std::path::PathBuf> {
    let cache_dir = dirs::cache_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    Ok(cache_dir.join("zammad-tui").join("debug.log"))
}

fn now_str() -> String {
    // Use system time formatted as HH:MM:SS.mmm
    let now = std::time::SystemTime::now();
    let dur = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let millis = dur.subsec_millis();
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

fn truncate_log(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.replace('\n', "\\n")
    } else {
        format!(
            "{}... ({} bytes total)",
            &s[..max].replace('\n', "\\n"),
            s.len()
        )
    }
}

/// Simple percent-encoding for query parameter values.
fn urlencoded(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push('+'),
            _ => {
                let mut buf = [0u8; 4];
                let encoded = c.encode_utf8(&mut buf);
                for b in encoded.bytes() {
                    result.push_str(&format!("%{b:02X}"));
                }
            }
        }
    }
    result
}
