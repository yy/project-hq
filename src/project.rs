use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::NaiveDate;

use crate::frontmatter::parse_frontmatter;

pub const DEFAULT_PRIORITY: i32 = 50;

#[derive(Debug, serde::Serialize)]
pub struct Project {
    pub title: String,
    pub track: String,
    pub status: String,
    pub owner: String,
    pub priority: i32,
    pub waiting_on: String,
    pub waiting_since: Option<NaiveDate>,
    pub my_next: String,
    pub last: String,
    pub deadline: Option<String>,
    pub deferred_until: Option<NaiveDate>,
    pub file: String,
}

impl Project {
    pub fn from_file(path: &Path, track: &str, hq_dir: &Path) -> Option<Self> {
        let text = fs::read_to_string(path).ok()?;
        let file = path
            .strip_prefix(hq_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        Self::from_text(&text, track, &file)
    }

    /// Parse a project directly from markdown text plus its logical file path.
    pub fn from_text(text: &str, track: &str, file: &str) -> Option<Self> {
        let fields = parse_frontmatter(text)?;
        Self::from_fields(&fields, track, file)
    }

    fn from_fields(fields: &BTreeMap<String, String>, track: &str, file: &str) -> Option<Self> {
        let priority = fields
            .get("priority")
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(DEFAULT_PRIORITY);

        Some(Self {
            title: fields.get("title")?.to_string(),
            track: fields
                .get("track")
                .map(|s| s.to_string())
                .unwrap_or_else(|| track.to_string()),
            status: fields.get("status")?.to_string(),
            owner: fields.get("owner").cloned().unwrap_or_default(),
            priority,
            waiting_on: fields.get("waiting_on").cloned().unwrap_or_default(),
            waiting_since: parse_date_field(fields, "waiting_since"),
            my_next: fields.get("my_next").cloned().unwrap_or_default(),
            last: fields.get("last").cloned().unwrap_or_default(),
            deadline: fields.get("deadline").cloned(),
            deferred_until: parse_date_field(fields, "deferred_until"),
            file: file.to_string(),
        })
    }

    pub fn deferred_days_past(&self) -> Option<i64> {
        self.deferred_until.and_then(non_negative_days_since)
    }

    pub fn waiting_days(&self) -> Option<i64> {
        self.waiting_since.and_then(non_negative_days_since)
    }

    pub fn is_waiting_like(&self) -> bool {
        matches!(self.status.as_str(), "waiting" | "submitted")
    }
}

fn parse_date_field(fields: &BTreeMap<String, String>, key: &str) -> Option<NaiveDate> {
    fields
        .get(key)
        .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
}

fn non_negative_days_since(date: NaiveDate) -> Option<i64> {
    let diff = (chrono::Local::now().date_naive() - date).num_days();
    (diff >= 0).then_some(diff)
}
