use std::path::Path;

use crate::project::DEFAULT_PRIORITY;
use crate::project_file::{rewrite_frontmatter_file, ProjectFileError};

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

pub fn move_project(hq_dir: &Path, opts: &MoveOptions) -> Result<(), ProjectFileError> {
    rewrite_frontmatter_file(hq_dir, &opts.file, |mut lines| {
        let status_line = field_line("status", &opts.to_status);
        let status_found = replace_field(&mut lines, "status", &status_line);

        if !status_found {
            return Err(ProjectFileError::missing_field(&opts.file, "status"));
        }

        if let Some(p) = opts.priority {
            let priority_line = field_line("priority", p);
            let priority_found = replace_field(&mut lines, "priority", &priority_line);

            if !priority_found && p != DEFAULT_PRIORITY {
                insert_field_after(&mut lines, "status", priority_line);
            }
        }

        Ok(lines)
    })
}

/// Set priority on a single file's frontmatter.
fn set_priority(hq_dir: &Path, file: &str, priority: i32) -> Result<(), ProjectFileError> {
    rewrite_frontmatter_file(hq_dir, file, |mut lines| {
        let priority_line = field_line("priority", priority);

        if !replace_field(&mut lines, "priority", &priority_line) {
            insert_field_after(&mut lines, "status", priority_line);
        }

        Ok(lines)
    })
}

/// Assign descending priorities to an ordered list of files.
/// First item gets highest priority (top of board).
pub fn reorder_projects(hq_dir: &Path, files: &[String]) -> Result<(), ProjectFileError> {
    let n = files.len();
    for (i, file) in files.iter().enumerate() {
        let priority = ((n - i) * 10) as i32;
        set_priority(hq_dir, file, priority)?;
    }
    Ok(())
}
