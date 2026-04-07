use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Ticket {
    pub id: u64,
    pub number: String,
    pub title: String,
    pub group_id: u64,
    pub state_id: u64,
    pub priority_id: u64,
    #[serde(default)]
    pub owner_id: u64,
    pub customer_id: u64,
    #[serde(default)]
    pub organization_id: Option<u64>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub close_at: Option<String>,
    #[serde(default)]
    pub article_count: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TicketArticle {
    pub id: u64,
    pub ticket_id: u64,
    #[serde(rename = "type", default)]
    pub article_type: Option<String>,
    #[serde(default)]
    pub sender: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub cc: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    pub body: String,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub internal: bool,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub created_by_id: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Attachment {
    pub id: u64,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub size: Option<serde_json::Value>,
    #[serde(default)]
    pub preferences: Option<AttachmentPreferences>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AttachmentPreferences {
    #[serde(rename = "Mime-Type", default)]
    pub mime_type: Option<String>,
    #[serde(rename = "Content-ID", default)]
    pub content_id: Option<String>,
    #[serde(rename = "Content-Disposition", default)]
    pub content_disposition: Option<String>,
}

impl Attachment {
    pub fn mime_type(&self) -> &str {
        self.preferences
            .as_ref()
            .and_then(|p| p.mime_type.as_deref())
            .unwrap_or("application/octet-stream")
    }

    pub fn is_inline(&self) -> bool {
        self.preferences
            .as_ref()
            .and_then(|p| p.content_disposition.as_deref())
            .is_some_and(|d| d == "inline")
    }

    pub fn is_image(&self) -> bool {
        self.mime_type().starts_with("image/")
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    pub id: u64,
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub firstname: Option<String>,
    #[serde(default)]
    pub lastname: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

impl User {
    pub fn display_name(&self) -> String {
        let first = self.firstname.as_deref().unwrap_or("");
        let last = self.lastname.as_deref().unwrap_or("");
        let name = format!("{first} {last}");
        let name = name.trim();
        if name.is_empty() {
            self.email
                .clone()
                .or(self.login.clone())
                .unwrap_or_else(|| format!("User #{}", self.id))
        } else {
            name.to_string()
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Group {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub active: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TicketState {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub state_type_id: Option<u64>,
    #[serde(default)]
    pub active: Option<bool>,
}

impl TicketState {
    pub fn is_closed(&self) -> bool {
        // Zammad closed states typically have state_type_id 5 (closed) or 6 (merged)
        // But we'll check by name as a fallback
        let name = self.name.to_lowercase();
        name == "closed" || name == "merged" || name == "resolved" || name.contains("closed")
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TicketPriority {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub active: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct CreateArticle {
    pub ticket_id: u64,
    #[serde(rename = "type")]
    pub article_type: String,
    pub subject: String,
    pub body: String,
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<String>,
    pub internal: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateTicket {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<u64>,
}
