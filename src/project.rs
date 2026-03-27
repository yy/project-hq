use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::NaiveDate;

#[derive(Debug)]
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
        let fields = parse_frontmatter(&text)?;

        let title = fields.get("title")?.to_string();
        let status = fields.get("status")?.to_string();

        let priority = fields
            .get("priority")
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(50);

        let waiting_since = fields
            .get("waiting_since")
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        let deferred_until = fields
            .get("deferred_until")
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        let file = path
            .strip_prefix(hq_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        Some(Project {
            title,
            track: fields
                .get("track")
                .map(|s| s.to_string())
                .unwrap_or_else(|| track.to_string()),
            status,
            owner: fields.get("owner").cloned().unwrap_or_default(),
            priority,
            waiting_on: fields.get("waiting_on").cloned().unwrap_or_default(),
            waiting_since,
            my_next: fields.get("my_next").cloned().unwrap_or_default(),
            last: fields.get("last").cloned().unwrap_or_default(),
            deadline: fields.get("deadline").cloned(),
            deferred_until,
            file,
        })
    }

    pub fn deferred_days_past(&self) -> Option<i64> {
        let until = self.deferred_until?;
        let today = chrono::Local::now().date_naive();
        let diff = (today - until).num_days();
        if diff >= 0 { Some(diff) } else { None }
    }

    pub fn waiting_days(&self) -> Option<i64> {
        let since = self.waiting_since?;
        let today = chrono::Local::now().date_naive();
        Some((today - since).num_days())
    }
}

fn parse_frontmatter(text: &str) -> Option<BTreeMap<String, String>> {
    if !text.starts_with("---") {
        return None;
    }
    let end = text[3..].find("---")?;
    let fm_text = &text[3..3 + end].trim();

    let mut fields = BTreeMap::new();
    for line in fm_text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().trim_matches('"').to_string();
            if !value.is_empty() {
                fields.insert(key, value);
            }
        }
    }

    if fields.contains_key("title") && fields.contains_key("status") {
        Some(fields)
    } else {
        None
    }
}
