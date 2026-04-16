use std::path::Path;

use crate::project::DEFAULT_PRIORITY;
use crate::project_file::{rewrite_frontmatter_file, validate_project_file, ProjectFileError};

pub struct MoveOptions {
    pub file: String,
    pub to_status: String,
    pub priority: Option<i32>,
}

struct FrontmatterLines {
    lines: Vec<String>,
}

impl FrontmatterLines {
    fn new(lines: Vec<String>) -> Self {
        Self { lines }
    }

    fn into_inner(self) -> Vec<String> {
        self.lines
    }

    fn replace(&mut self, field: &str, value: impl std::fmt::Display) -> bool {
        let replacement = format!("{field}: {value}");
        self.replace_line(field, &replacement)
    }

    fn replace_line(&mut self, field: &str, replacement: &str) -> bool {
        let mut found = false;

        for line in &mut self.lines {
            if matches_field(line, field) {
                *line = replacement.to_string();
                found = true;
            }
        }

        found
    }

    fn upsert_after(&mut self, field: &str, value: impl std::fmt::Display, anchor: &str) {
        let new_line = format!("{field}: {value}");
        if !self.replace_line(field, &new_line) {
            if let Some(pos) = self
                .lines
                .iter()
                .position(|line| matches_field(line, anchor))
            {
                self.lines.insert(pos + 1, new_line);
            } else {
                self.lines.push(new_line);
            }
        }
    }
}

fn matches_field(line: &str, field: &str) -> bool {
    line.trim_start()
        .strip_prefix(field)
        .is_some_and(|rest| rest.starts_with(':'))
}

pub fn move_project(hq_dir: &Path, opts: &MoveOptions) -> Result<(), ProjectFileError> {
    rewrite_frontmatter_file(hq_dir, &opts.file, |mut lines| {
        let mut frontmatter = FrontmatterLines::new(std::mem::take(&mut lines));

        if !frontmatter.replace("status", &opts.to_status) {
            return Err(ProjectFileError::missing_field(&opts.file, "status"));
        }

        if let Some(p) = opts.priority {
            if p == DEFAULT_PRIORITY {
                frontmatter.replace("priority", p);
            } else {
                frontmatter.upsert_after("priority", p, "status");
            }
        }

        Ok(frontmatter.into_inner())
    })
}

/// Set priority on a single file's frontmatter.
fn set_priority(hq_dir: &Path, file: &str, priority: i32) -> Result<(), ProjectFileError> {
    rewrite_frontmatter_file(hq_dir, file, |mut lines| {
        let mut frontmatter = FrontmatterLines::new(std::mem::take(&mut lines));
        frontmatter.upsert_after("priority", priority, "status");
        Ok(frontmatter.into_inner())
    })
}

/// Assign descending priorities to an ordered list of files.
/// First item gets highest priority (top of board).
pub fn reorder_projects(hq_dir: &Path, files: &[String]) -> Result<(), ProjectFileError> {
    for file in files {
        validate_project_file(hq_dir, file)?;
    }

    let n = files.len();
    for (i, file) in files.iter().enumerate() {
        let priority = ((n - i) * 10) as i32;
        set_priority(hq_dir, file, priority)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::FrontmatterLines;

    #[test]
    fn replace_updates_all_matching_fields() {
        let mut frontmatter = FrontmatterLines::new(vec![
            "title: Project".to_string(),
            "status: active".to_string(),
            " status: deferred".to_string(),
        ]);

        assert!(frontmatter.replace("status", "waiting"));
        assert_eq!(
            frontmatter.into_inner(),
            vec!["title: Project", "status: waiting", "status: waiting",]
        );
    }

    #[test]
    fn upsert_after_appends_when_anchor_is_missing() {
        let mut frontmatter = FrontmatterLines::new(vec!["title: Project".to_string()]);

        frontmatter.upsert_after("priority", 70, "status");

        assert_eq!(
            frontmatter.into_inner(),
            vec!["title: Project", "priority: 70"]
        );
    }
}
