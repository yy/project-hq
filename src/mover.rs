use std::path::Path;

use crate::action::ActionMode;
use crate::project::{valid_deferred_until, DEFAULT_PRIORITY};
use crate::project_file::{
    rewrite_frontmatter_fields, validate_project_file_for_rewrite, ProjectFileError,
};

pub struct MoveOptions {
    pub file: String,
    pub to_status: String,
    pub priority: Option<f64>,
}

pub struct MetadataOptions {
    pub file: String,
    pub title: String,
    pub status: String,
    pub priority: f64,
    pub owner: String,
    pub my_next: String,
    pub waiting_on: String,
    pub waiting_since: String,
    pub deadline: String,
    pub deferred_until: String,
    pub action_mode: String,
}

fn validate_status(file: &str, status: &str) -> Result<(), ProjectFileError> {
    if status.trim().is_empty() {
        return Err(ProjectFileError::InvalidStatus {
            file: file.to_string(),
        });
    }

    Ok(())
}

fn is_default_priority(priority: f64) -> bool {
    (priority - DEFAULT_PRIORITY).abs() < f64::EPSILON
}

pub fn move_project(hq_dir: &Path, opts: &MoveOptions) -> Result<(), ProjectFileError> {
    validate_status(&opts.file, &opts.to_status)?;

    rewrite_frontmatter_fields(hq_dir, &opts.file, |frontmatter| {
        if !frontmatter.replace("status", &opts.to_status) {
            return Err(ProjectFileError::missing_field(&opts.file, "status"));
        }

        if let Some(p) = opts.priority {
            if is_default_priority(p) {
                frontmatter.replace("priority", p);
            } else {
                frontmatter.upsert_after("priority", p, "status");
            }
        }

        Ok(())
    })
}

pub fn defer_project(hq_dir: &Path, file: &str, until: &str) -> Result<(), ProjectFileError> {
    if !valid_deferred_until(until) {
        return Err(ProjectFileError::InvalidDate {
            file: file.to_string(),
            field: "deferred_until",
        });
    }

    rewrite_frontmatter_fields(hq_dir, file, |frontmatter| {
        frontmatter.upsert_after("deferred_until", until, "status");
        Ok(())
    })
}

pub fn update_project_metadata(
    hq_dir: &Path,
    options: &MetadataOptions,
) -> Result<(), ProjectFileError> {
    if options.title.trim().is_empty() {
        return Err(ProjectFileError::missing_field(&options.file, "title"));
    }
    validate_status(&options.file, &options.status)?;
    if !options.priority.is_finite() {
        return Err(ProjectFileError::Frontmatter {
            file: options.file.clone(),
            reason: "Invalid priority",
        });
    }
    if !options.deferred_until.is_empty() && !valid_deferred_until(&options.deferred_until) {
        return Err(ProjectFileError::InvalidDate {
            file: options.file.clone(),
            field: "deferred_until",
        });
    }
    let action_mode =
        ActionMode::parse(&options.action_mode).ok_or_else(|| ProjectFileError::Frontmatter {
            file: options.file.clone(),
            reason: "Invalid action_mode",
        })?;

    rewrite_frontmatter_fields(hq_dir, &options.file, |frontmatter| {
        if !frontmatter.replace_string("title", &options.title) {
            return Err(ProjectFileError::missing_field(&options.file, "title"));
        }
        if !frontmatter.replace_string("status", &options.status) {
            return Err(ProjectFileError::missing_field(&options.file, "status"));
        }
        frontmatter.upsert_after("priority", options.priority, "status");

        for (field, value, anchor) in [
            ("owner", options.owner.as_str(), "title"),
            ("my_next", options.my_next.as_str(), "priority"),
            ("waiting_on", options.waiting_on.as_str(), "status"),
            (
                "waiting_since",
                options.waiting_since.as_str(),
                "waiting_on",
            ),
            ("deadline", options.deadline.as_str(), "priority"),
            ("deferred_until", options.deferred_until.as_str(), "status"),
            ("action_mode", action_mode.as_str(), "status"),
        ] {
            if value.is_empty() {
                frontmatter.remove(field);
            } else {
                frontmatter.upsert_string_after(field, value, anchor);
            }
        }
        Ok(())
    })
}

/// Set priority on a single file's frontmatter.
fn set_priority(hq_dir: &Path, file: &str, priority: f64) -> Result<(), ProjectFileError> {
    rewrite_frontmatter_fields(hq_dir, file, |frontmatter| {
        frontmatter.upsert_after("priority", priority, "status");
        Ok(())
    })
}

/// Assign descending priorities to an ordered list of files.
/// First item gets highest priority (top of board).
pub fn reorder_projects(hq_dir: &Path, files: &[String]) -> Result<(), ProjectFileError> {
    for file in files {
        validate_project_file_for_rewrite(hq_dir, file)?;
    }

    let n = files.len();
    for (i, file) in files.iter().enumerate() {
        let priority = ((n - i) * 10) as f64;
        set_priority(hq_dir, file, priority)?;
    }
    Ok(())
}
