use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::{DateTime, FixedOffset, Local, NaiveDate};

use crate::action::{parse_actions, Action, ActionMode};
use crate::frontmatter::parse_frontmatter;
use crate::project_file::project_body;

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
    pub deferred_until: Option<String>,
    pub visible: bool,
    pub action_mode: ActionMode,
    pub actions: Vec<Action>,
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
        Self::from_fields(&fields, project_body(text), track, file)
    }

    fn from_fields(
        fields: &BTreeMap<String, String>,
        body: &str,
        track: &str,
        file: &str,
    ) -> Option<Self> {
        let fields = ProjectFields::new(fields);
        let status = fields.text("status")?;
        let my_next = fields.text_or_default("my_next");
        let deferred_until = fields.text("deferred_until");
        let visible =
            deferred_is_visible_at(deferred_until.as_deref(), Local::now().fixed_offset());
        let action_mode = fields.action_mode();
        let actions = parse_actions(
            body,
            (!my_next.trim().is_empty() && my_next.trim() != "(fill in)")
                .then_some(my_next.as_str()),
            action_mode,
            visible && matches!(status.as_str(), "active" | "my-plate"),
        );

        Some(Self {
            title: fields.text("title")?,
            track: fields.text("track").unwrap_or_else(|| track.to_string()),
            status,
            owner: fields.text_or_default("owner"),
            priority: fields.priority(),
            waiting_on: fields.text_or_default("waiting_on"),
            waiting_since: fields.date("waiting_since"),
            my_next,
            last: fields.text_or_default("last"),
            deadline: fields.text("deadline"),
            deferred_until,
            visible,
            action_mode,
            actions,
            file: file.to_string(),
        })
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

    fn action_mode(&self) -> ActionMode {
        ActionMode::from_field(self.fields.get("action_mode").map(String::as_str))
    }
}

fn non_negative_days_since(date: NaiveDate) -> Option<i64> {
    let diff = (chrono::Local::now().date_naive() - date).num_days();
    (diff >= 0).then_some(diff)
}

enum DeferredUntil {
    Date(NaiveDate),
    Timestamp(DateTime<FixedOffset>),
}

fn parse_deferred_until(value: &str) -> Option<DeferredUntil> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(DeferredUntil::Date)
        .or_else(|_| DateTime::parse_from_rfc3339(value).map(DeferredUntil::Timestamp))
        .ok()
}

pub(crate) fn valid_deferred_until(value: &str) -> bool {
    parse_deferred_until(value).is_some()
}

fn deferred_is_visible_at(value: Option<&str>, now: DateTime<FixedOffset>) -> bool {
    value.is_none_or(|value| match parse_deferred_until(value) {
        Some(DeferredUntil::Date(date)) => date <= now.date_naive(),
        Some(DeferredUntil::Timestamp(timestamp)) => timestamp <= now,
        None => true,
    })
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;

    use crate::action::ActionMode;

    use super::{deferred_is_visible_at, valid_deferred_until, Project, DEFAULT_PRIORITY};

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
            visible: true,
            action_mode: ActionMode::Parallel,
            actions: Vec::new(),
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
        assert!(project.visible);
        assert_eq!(project.action_mode, ActionMode::Parallel);
        assert!(project.actions.is_empty());
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

    #[test]
    fn future_deferral_hides_project_and_its_actions() {
        let project = Project::from_text(
            "---\n\
title: Future\n\
status: active\n\
deferred_until: 2999-01-01\n\
---\n\
\n\
- [ ] Call Mom @phone\n",
            "personal",
            "personal/future.md",
        )
        .unwrap();

        assert!(!project.visible);
        assert!(project.actions.iter().all(|action| !action.available));
    }

    #[test]
    fn elapsed_deferral_is_visible_without_status_change() {
        let project = Project::from_text(
            "---\n\
title: Ready\n\
status: active\n\
deferred_until: 2000-01-01\n\
---\n\
\n\
- [ ] Call Mom @phone\n",
            "personal",
            "personal/ready.md",
        )
        .unwrap();

        assert!(project.visible);
        assert_eq!(project.status, "active");
        assert!(project.actions[0].available);
    }

    #[test]
    fn deferral_becomes_visible_on_its_date() {
        let now = DateTime::parse_from_rfc3339("2026-07-26T12:00:00-04:00").unwrap();

        assert!(!deferred_is_visible_at(Some("2026-07-27"), now));
        assert!(deferred_is_visible_at(Some("2026-07-26"), now));
        assert!(deferred_is_visible_at(None, now));
    }

    #[test]
    fn timestamp_deferral_becomes_visible_at_the_exact_instant() {
        let before = DateTime::parse_from_rfc3339("2026-07-26T12:59:59-04:00").unwrap();
        let exact = DateTime::parse_from_rfc3339("2026-07-26T13:00:00-04:00").unwrap();

        assert!(!deferred_is_visible_at(
            Some("2026-07-26T17:00:00Z"),
            before
        ));
        assert!(deferred_is_visible_at(Some("2026-07-26T17:00:00Z"), exact));
    }

    #[test]
    fn validates_date_and_rfc3339_deferrals() {
        assert!(valid_deferred_until("2026-07-27"));
        assert!(valid_deferred_until("2026-07-26T17:00:00.000Z"));
        assert!(!valid_deferred_until("tomorrow"));
        assert!(!valid_deferred_until("2026-02-30"));
    }
}
