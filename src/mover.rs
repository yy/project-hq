use std::fs;
use std::path::Path;

pub struct MoveOptions {
    pub file: String,
    pub to_status: String,
    pub priority: Option<i32>,
}

/// Find the end index of the frontmatter closing `---` (must be on its own line).
/// Returns (fm_text, body) slices on success.
fn split_frontmatter(text: &str) -> Result<(&str, &str), &'static str> {
    if !text.starts_with("---") {
        return Err("No frontmatter");
    }
    let rest = &text[3..];
    let end = rest
        .match_indices("---")
        .find(|(i, _)| *i == 0 || rest.as_bytes().get(i - 1) == Some(&b'\n'))
        .map(|(i, _)| i)
        .ok_or("Malformed frontmatter")?;
    let fm_text = &rest[..end];
    let body = &rest[end + 3..];
    Ok((fm_text, body))
}

pub fn move_project(hq_dir: &Path, opts: &MoveOptions) -> Result<(), String> {
    let filepath = hq_dir.join(&opts.file);
    let text = fs::read_to_string(&filepath)
        .map_err(|e| format!("{}: {e}", opts.file))?;

    let (fm_text, body) = split_frontmatter(&text)
        .map_err(|e| format!("{} in {}", e, opts.file))?;

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
    let result = format!("---{new_fm}\n---{body}");
    fs::write(&filepath, result).map_err(|e| format!("Write failed: {e}"))?;
    Ok(())
}

/// Set priority on a single file's frontmatter.
fn set_priority(hq_dir: &Path, file: &str, priority: i32) -> Result<(), String> {
    let filepath = hq_dir.join(file);
    let text = fs::read_to_string(&filepath).map_err(|e| format!("{file}: {e}"))?;

    let (fm_text, body) = split_frontmatter(&text)
        .map_err(|e| format!("{e} in {file}"))?;

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
    let result = format!("---{new_fm}\n---{body}");
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
