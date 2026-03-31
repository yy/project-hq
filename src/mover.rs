use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::frontmatter::split_frontmatter;

pub struct MoveOptions {
    pub file: String,
    pub to_status: String,
    pub priority: Option<i32>,
}

fn resolve_project_path(hq_dir: &Path, file: &str) -> Result<PathBuf, String> {
    let path = Path::new(file);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("Invalid file path: {file}"));
    }
    Ok(hq_dir.join(path))
}

fn rewrite_frontmatter_file(
    hq_dir: &Path,
    file: &str,
    rewrite: impl FnOnce(Vec<String>) -> Result<Vec<String>, String>,
) -> Result<(), String> {
    let filepath = resolve_project_path(hq_dir, file)?;
    let text = fs::read_to_string(&filepath).map_err(|e| format!("{file}: {e}"))?;
    let (fm_text, body) = split_frontmatter(&text).map_err(|e| format!("{e} in {file}"))?;

    let lines = fm_text.lines().map(str::to_string).collect();
    let new_fm = rewrite(lines)?.join("\n");
    let result = format!("---{new_fm}\n---{body}");
    fs::write(&filepath, result).map_err(|e| format!("Write failed: {e}"))?;
    Ok(())
}

pub fn move_project(hq_dir: &Path, opts: &MoveOptions) -> Result<(), String> {
    rewrite_frontmatter_file(hq_dir, &opts.file, |mut lines| {
        let mut status_found = false;
        let mut priority_found = false;

        for line in &mut lines {
            if line.trim_start().starts_with("status:") {
                *line = format!("status: {}", opts.to_status);
                status_found = true;
            } else if line.trim_start().starts_with("priority:") {
                if let Some(p) = opts.priority {
                    *line = format!("priority: {p}");
                }
                priority_found = true;
            }
        }

        if !status_found {
            return Err(format!("No status field in {}", opts.file));
        }

        if let Some(p) = opts.priority.filter(|_| !priority_found) {
            if p != 50 {
                if let Some(pos) = lines.iter().position(|l| l.starts_with("status:")) {
                    lines.insert(pos + 1, format!("priority: {p}"));
                }
            }
        }

        Ok(lines)
    })
}

/// Set priority on a single file's frontmatter.
fn set_priority(hq_dir: &Path, file: &str, priority: i32) -> Result<(), String> {
    rewrite_frontmatter_file(hq_dir, file, |mut lines| {
        let mut priority_found = false;

        for line in &mut lines {
            if line.trim_start().starts_with("priority:") {
                *line = format!("priority: {priority}");
                priority_found = true;
            }
        }

        if !priority_found {
            if let Some(pos) = lines
                .iter()
                .position(|l| l.trim_start().starts_with("status:"))
            {
                lines.insert(pos + 1, format!("priority: {priority}"));
            } else {
                lines.push(format!("priority: {priority}"));
            }
        }

        Ok(lines)
    })
}

/// Assign descending priorities to an ordered list of files.
/// First item gets highest priority (top of board).
pub fn reorder_projects(hq_dir: &Path, files: &[String]) -> Result<(), String> {
    let n = files.len();
    for (i, file) in files.iter().enumerate() {
        let priority = ((n - i) * 10) as i32;
        set_priority(hq_dir, file, priority)?;
    }
    Ok(())
}
