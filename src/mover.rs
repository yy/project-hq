use std::fs;
use std::path::Path;

pub struct MoveOptions {
    pub file: String,
    pub to_status: String,
    pub priority: Option<i32>,
}

pub fn move_project(hq_dir: &Path, opts: &MoveOptions) -> Result<(), String> {
    let filepath = hq_dir.join(&opts.file);
    let text = fs::read_to_string(&filepath)
        .map_err(|e| format!("{}: {e}", opts.file))?;

    if !text.starts_with("---") {
        return Err(format!("No frontmatter in {}", opts.file));
    }
    let fm_end = text[3..]
        .find("---")
        .ok_or_else(|| format!("Malformed frontmatter in {}", opts.file))?;

    let fm_text = &text[3..3 + fm_end];
    let body = &text[3 + fm_end + 3..];

    let mut lines: Vec<String> = Vec::new();
    let mut status_found = false;
    let mut priority_found = false;

    for line in fm_text.lines() {
        if line.trim_start().starts_with("status:") {
            lines.push(format!("status: {}", opts.to_status));
            status_found = true;
        } else if line.trim_start().starts_with("priority:") {
            if let Some(p) = opts.priority {
                lines.push(format!("priority: {p}"));
            } else {
                lines.push(line.to_string());
            }
            priority_found = true;
        } else {
            lines.push(line.to_string());
        }
    }

    if !status_found {
        return Err(format!("No status field in {}", opts.file));
    }

    // Insert priority after status if it was specified but didn't exist
    if opts.priority.is_some() && !priority_found {
        let p = opts.priority.unwrap();
        if p != 50 {
            if let Some(pos) = lines.iter().position(|l| l.starts_with("status:")) {
                lines.insert(pos + 1, format!("priority: {p}"));
            }
        }
    }

    let new_fm = lines.join("\n");
    let result = format!("---{new_fm}---{body}");
    fs::write(&filepath, result).map_err(|e| format!("Write failed: {e}"))?;
    Ok(())
}

/// Set priority on a single file's frontmatter.
fn set_priority(hq_dir: &Path, file: &str, priority: i32) -> Result<(), String> {
    let filepath = hq_dir.join(file);
    let text = fs::read_to_string(&filepath).map_err(|e| format!("{file}: {e}"))?;

    if !text.starts_with("---") {
        return Err(format!("No frontmatter in {file}"));
    }
    let fm_end = text[3..]
        .find("---")
        .ok_or_else(|| format!("Malformed frontmatter in {file}"))?;

    let fm_text = &text[3..3 + fm_end];
    let body = &text[3 + fm_end + 3..];

    let mut lines: Vec<String> = Vec::new();
    let mut priority_found = false;

    for line in fm_text.lines() {
        if line.trim_start().starts_with("priority:") {
            lines.push(format!("priority: {priority}"));
            priority_found = true;
        } else {
            lines.push(line.to_string());
        }
    }

    if !priority_found {
        // Insert after status line
        if let Some(pos) = lines.iter().position(|l| l.trim_start().starts_with("status:")) {
            lines.insert(pos + 1, format!("priority: {priority}"));
        } else {
            lines.push(format!("priority: {priority}"));
        }
    }

    let new_fm = lines.join("\n");
    let result = format!("---{new_fm}---{body}");
    fs::write(&filepath, result).map_err(|e| format!("Write failed: {e}"))?;
    Ok(())
}

/// Assign sequential priorities (10, 20, 30, ...) to an ordered list of files.
pub fn reorder_projects(hq_dir: &Path, files: &[String]) -> Result<(), String> {
    for (i, file) in files.iter().enumerate() {
        let priority = ((i + 1) * 10) as i32;
        set_priority(hq_dir, file, priority)?;
    }
    Ok(())
}
