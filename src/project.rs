use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::NaiveDate;

use crate::frontmatter::parse_frontmatter;

pub const DEFAULT_PRIORITY: f64 = 50.0;

#[derive(Debug, serde::Serialize)]
pub struct Project {
    pub title: String,
    pub track: String,
    pub status: String,
    pub owner: String,
    pub priority: f64,
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
        let fields = ProjectFields::new(fields);

        Some(Self {
            title: fields.text("title")?,
            track: fields.text("track").unwrap_or_else(|| track.to_string()),
            status: fields.text("status")?,
            owner: fields.text_or_default("owner"),
            priority: fields.priority(),
            waiting_on: fields.text_or_default("waiting_on"),
            waiting_since: fields.date("waiting_since"),
            my_next: fields.text_or_default("my_next"),
            last: fields.text_or_default("last"),
            deadline: fields.text("deadline"),
            deferred_until: fields.date("deferred_until"),
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

    pub fn actionable_next_step(&self) -> Option<&str> {
        let next = self.my_next.trim();
        (!next.is_empty() && next != "(fill in)").then_some(next)
    }
}

struct ProjectFields<'a> {
    fields: &'a BTreeMap<String, String>,
}

impl<'a> ProjectFields<'a> {
    fn new(fields: &'a BTreeMap<String, String>) -> Self {
        Self { fields }
    }

    fn text(&self, key: &str) -> Option<String> {
        self.fields.get(key).cloned()
    }

    fn text_or_default(&self, key: &str) -> String {
        self.text(key).unwrap_or_default()
    }

    fn date(&self, key: &str) -> Option<NaiveDate> {
        self.fields
            .get(key)
            .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
    }

    fn priority(&self) -> f64 {
        self.fields
            .get("priority")
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|priority| priority.is_finite())
            .unwrap_or(DEFAULT_PRIORITY)
    }
}

fn non_negative_days_since(date: NaiveDate) -> Option<i64> {
    let diff = (chrono::Local::now().date_naive() - date).num_days();
    (diff >= 0).then_some(diff)
}

#[cfg(test)]
mod tests {
    use super::{Project, DEFAULT_PRIORITY};

    fn project_with_next_step(my_next: &str) -> Project {
        Project {
            title: "Project".to_string(),
            track: "research".to_string(),
            status: "active".to_string(),
            owner: String::new(),
            priority: DEFAULT_PRIORITY,
            waiting_on: String::new(),
            waiting_since: None,
            my_next: my_next.to_string(),
            last: String::new(),
            deadline: None,
            deferred_until: None,
            file: "research/project.md".to_string(),
        }
    }

    #[test]
    fn from_text_defaults_missing_optional_fields() {
        let project = Project::from_text(
            "---\ntitle: Project\nstatus: active\n---\n",
            "research",
            "research/project.md",
        )
        .unwrap();

        assert_eq!(project.title, "Project");
        assert_eq!(project.track, "research");
        assert_eq!(project.status, "active");
        assert_eq!(project.owner, "");
        assert_eq!(project.priority, DEFAULT_PRIORITY);
        assert_eq!(project.waiting_on, "");
        assert_eq!(project.waiting_since, None);
        assert_eq!(project.my_next, "");
        assert_eq!(project.last, "");
        assert_eq!(project.deadline, None);
        assert_eq!(project.deferred_until, None);
        assert_eq!(project.file, "research/project.md");
    }

    #[test]
    fn actionable_next_step_ignores_blank_and_placeholder_values() {
        assert_eq!(project_with_next_step("").actionable_next_step(), None);
        assert_eq!(project_with_next_step("   ").actionable_next_step(), None);
        assert_eq!(
            project_with_next_step("(fill in)").actionable_next_step(),
            None
        );
    }

    #[test]
    fn actionable_next_step_trims_real_values() {
        assert_eq!(
            project_with_next_step("  draft intro  ").actionable_next_step(),
            Some("draft intro")
        );
    }
}
