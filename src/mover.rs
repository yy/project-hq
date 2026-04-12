use std::path::Path;

use crate::project::DEFAULT_PRIORITY;
use crate::project_file::{rewrite_frontmatter_file, validate_project_file, ProjectFileError};

pub struct MoveOptions {
    pub file: String,
    pub to_status: String,
    pub priority: Option<i32>,
}

fn field_line(field: &str, value: impl std::fmt::Display) -> String {
    format!("{field}: {value}")
}

fn matches_field(line: &str, field: &str) -> bool {
    line.trim_start()
        .strip_prefix(field)
        .is_some_and(|rest| rest.starts_with(':'))
}

fn replace_field(lines: &mut [String], field: &str, replacement: &str) -> bool {
    let mut found = false;

    for line in lines {
        if matches_field(line, field) {
            *line = replacement.to_string();
            found = true;
        }
    }

    found
}

fn insert_field_after(lines: &mut Vec<String>, anchor: &str, new_line: String) {
    if let Some(pos) = lines.iter().position(|line| matches_field(line, anchor)) {
        lines.insert(pos + 1, new_line);
    } else {
        lines.push(new_line);
    }
}

fn upsert_field(
    lines: &mut Vec<String>,
    field: &str,
    value: impl std::fmt::Display,
    insert_after: &str,
    insert_if_missing: bool,
) -> bool {
    let replacement = field_line(field, value);
    let found = replace_field(lines, field, &replacement);

    if !found && insert_if_missing {
        insert_field_after(lines, insert_after, replacement);
    }

    found
}

pub fn move_project(hq_dir: &Path, opts: &MoveOptions) -> Result<(), ProjectFileError> {
    rewrite_frontmatter_file(hq_dir, &opts.file, |mut lines| {
        let status_found = upsert_field(&mut lines, "status", &opts.to_status, "status", false);

        if !status_found {
            return Err(ProjectFileError::missing_field(&opts.file, "status"));
        }

        if let Some(p) = opts.priority {
            upsert_field(&mut lines, "priority", p, "status", p != DEFAULT_PRIORITY);
        }

        Ok(lines)
    })
}

/// Set priority on a single file's frontmatter.
fn set_priority(hq_dir: &Path, file: &str, priority: i32) -> Result<(), ProjectFileError> {
    rewrite_frontmatter_file(hq_dir, file, |mut lines| {
        upsert_field(&mut lines, "priority", priority, "status", true);

        Ok(lines)
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
