use std::path::Path;

use crate::project::DEFAULT_PRIORITY;
use crate::project_file::{rewrite_frontmatter_fields, validate_project_file, ProjectFileError};

pub struct MoveOptions {
    pub file: String,
    pub to_status: String,
    pub priority: Option<i32>,
}

pub fn move_project(hq_dir: &Path, opts: &MoveOptions) -> Result<(), ProjectFileError> {
    rewrite_frontmatter_fields(hq_dir, &opts.file, |frontmatter| {
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

        Ok(())
    })
}

/// Set priority on a single file's frontmatter.
fn set_priority(hq_dir: &Path, file: &str, priority: i32) -> Result<(), ProjectFileError> {
    rewrite_frontmatter_fields(hq_dir, file, |frontmatter| {
        frontmatter.upsert_after("priority", priority, "status");
        Ok(())
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
