use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::frontmatter::split_frontmatter;

#[derive(Debug)]
pub enum ProjectFileError {
    InvalidPath(String),
    Read { file: String, source: io::Error },
    Write { file: String, source: io::Error },
    Frontmatter { file: String, reason: &'static str },
    MissingField { file: String, field: &'static str },
    CheckboxConflict,
}

impl ProjectFileError {
    pub fn missing_field(file: &str, field: &'static str) -> Self {
        Self::MissingField {
            file: file.to_string(),
            field,
        }
    }

    pub fn is_bad_request(&self) -> bool {
        matches!(
            self,
            Self::InvalidPath(_) | Self::Frontmatter { .. } | Self::MissingField { .. }
        )
    }

    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound
        )
    }

    pub fn is_conflict(&self) -> bool {
        matches!(self, Self::CheckboxConflict)
    }
}

impl fmt::Display for ProjectFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(file) => write!(f, "Invalid file path: {file}"),
            Self::Read { file, source } => write!(f, "{file}: {source}"),
            Self::Write { file, source } => write!(f, "{file}: {source}"),
            Self::Frontmatter { file, reason } => write!(f, "{reason} in {file}"),
            Self::MissingField { file, field } => write!(f, "No {field} field in {file}"),
            Self::CheckboxConflict => write!(f, "Checkbox state has changed; reload and retry"),
        }
    }
}

impl std::error::Error for ProjectFileError {}

struct ProjectDocument {
    file: String,
    path: PathBuf,
    frontmatter: String,
    body_section: String,
}

pub(crate) struct FrontmatterLines {
    lines: Vec<String>,
}

impl FrontmatterLines {
    fn new(lines: Vec<String>) -> Self {
        Self { lines }
    }

    fn into_inner(self) -> Vec<String> {
        self.lines
    }

    pub(crate) fn replace(&mut self, field: &str, value: impl std::fmt::Display) -> bool {
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

    pub(crate) fn upsert_after(
        &mut self,
        field: &str,
        value: impl std::fmt::Display,
        anchor: &str,
    ) {
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
        .is_some_and(|rest| rest.trim_start().starts_with(':'))
}

impl ProjectDocument {
    fn read(hq_dir: &Path, file: &str) -> Result<Self, ProjectFileError> {
        let path = resolve_project_path(hq_dir, file)?;
        let text = fs::read_to_string(&path).map_err(|source| ProjectFileError::Read {
            file: file.to_string(),
            source,
        })?;
        let (frontmatter, body) = split_project_frontmatter(file, &text)?;

        Ok(Self {
            file: file.to_string(),
            path,
            frontmatter: frontmatter.to_string(),
            body_section: body.to_string(),
        })
    }

    fn body_text(&self) -> &str {
        strip_frontmatter_separators(&self.body_section)
    }

    fn write(&self, frontmatter: &str, body_section: &str) -> Result<(), ProjectFileError> {
        let result = assemble_project_text(frontmatter, body_section);

        fs::write(&self.path, result).map_err(|source| ProjectFileError::Write {
            file: self.file.clone(),
            source,
        })
    }

    fn write_body(&self, body: &str) -> Result<(), ProjectFileError> {
        self.write(&self.frontmatter, &normalize_body(body))
    }

    fn rewrite_frontmatter(
        &self,
        rewrite: impl FnOnce(Vec<String>) -> Result<Vec<String>, ProjectFileError>,
    ) -> Result<(), ProjectFileError> {
        let lines = self.frontmatter.lines().map(str::to_string).collect();
        let new_frontmatter = rewrite(lines)?.join("\n");
        self.write(&new_frontmatter, &self.body_section)
    }
}

fn assemble_project_text(frontmatter: &str, body: &str) -> String {
    let frontmatter = normalize_frontmatter(frontmatter);
    let frontmatter = frontmatter.as_str();
    format!("---{frontmatter}---{body}")
}

fn normalize_frontmatter(frontmatter: &str) -> String {
    let mut normalized = String::new();
    if !frontmatter.starts_with(['\n', '\r']) {
        normalized.push('\n');
    }
    normalized.push_str(frontmatter);
    if !frontmatter.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

fn normalize_body(body: &str) -> String {
    if body.is_empty() {
        "\n".to_string()
    } else {
        let mut normalized = format!("\n\n{body}");
        if !body.ends_with('\n') {
            normalized.push('\n');
        }
        normalized
    }
}

fn split_project_frontmatter<'a>(
    file: &str,
    text: &'a str,
) -> Result<(&'a str, &'a str), ProjectFileError> {
    split_frontmatter(text).map_err(|reason| ProjectFileError::Frontmatter {
        file: file.to_string(),
        reason,
    })
}

fn resolve_project_path(hq_dir: &Path, file: &str) -> Result<PathBuf, ProjectFileError> {
    let path = Path::new(file);
    if !file.ends_with(".md")
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ProjectFileError::InvalidPath(file.to_string()));
    }

    let resolved = hq_dir.join(path);
    let canonical_hq_dir = fs::canonicalize(hq_dir).unwrap_or_else(|_| hq_dir.to_path_buf());

    if let Ok(canonical_resolved) = fs::canonicalize(&resolved) {
        if !canonical_resolved.starts_with(&canonical_hq_dir) {
            return Err(ProjectFileError::InvalidPath(file.to_string()));
        }
    }

    Ok(resolved)
}

fn strip_frontmatter_separators(body: &str) -> &str {
    let body = body
        .strip_prefix("\r\n")
        .or_else(|| body.strip_prefix('\n'))
        .unwrap_or(body);

    body.strip_prefix("\r\n")
        .or_else(|| body.strip_prefix('\n'))
        .unwrap_or(body)
}

pub fn project_body(text: &str) -> &str {
    split_frontmatter(text)
        .map(|(_, body)| strip_frontmatter_separators(body))
        .unwrap_or(text)
}

pub fn validate_project_file(hq_dir: &Path, file: &str) -> Result<(), ProjectFileError> {
    ProjectDocument::read(hq_dir, file)?;
    Ok(())
}

pub fn read_project_body(hq_dir: &Path, file: &str) -> Result<String, ProjectFileError> {
    Ok(ProjectDocument::read(hq_dir, file)?.body_text().to_string())
}

pub fn write_project_body(hq_dir: &Path, file: &str, body: &str) -> Result<(), ProjectFileError> {
    ProjectDocument::read(hq_dir, file)?.write_body(body)
}

pub fn toggle_body_checkbox(
    hq_dir: &Path,
    file: &str,
    line_index: usize,
    expected_checked: bool,
    new_checked: bool,
) -> Result<(), ProjectFileError> {
    let doc = ProjectDocument::read(hq_dir, file)?;
    let body = doc.body_text().to_string();
    let mut lines: Vec<String> = body.split('\n').map(str::to_string).collect();
    let line = lines
        .get(line_index)
        .ok_or(ProjectFileError::CheckboxConflict)?;
    let toggled = toggle_checkbox_line(line, expected_checked, new_checked)
        .ok_or(ProjectFileError::CheckboxConflict)?;
    lines[line_index] = toggled;
    let new_body = lines.join("\n");
    doc.write_body(&new_body)
}

fn toggle_checkbox_line(line: &str, expected_checked: bool, new_checked: bool) -> Option<String> {
    let marker = CheckboxMarker::find(line)?;
    if marker.checked != expected_checked {
        return None;
    }

    Some(marker.rewrite(line, new_checked))
}

struct CheckboxMarker {
    start: usize,
    end: usize,
    checked: bool,
}

impl CheckboxMarker {
    fn find(line: &str) -> Option<Self> {
        let trimmed_start = line.len() - line.trim_start().len();
        let after_indent = &line[trimmed_start..];
        let bullet_len = after_indent
            .chars()
            .next()
            .filter(|c| matches!(c, '-' | '*' | '+'))
            .map(|c| c.len_utf8())?;
        let after_bullet = &after_indent[bullet_len..];
        let space_len = after_bullet.len() - after_bullet.trim_start_matches([' ', '\t']).len();
        if space_len == 0 {
            return None;
        }
        let after_spaces = &after_bullet[space_len..];
        let bracket = after_spaces.as_bytes();
        if bracket.len() < 3 || bracket[0] != b'[' || bracket[2] != b']' {
            return None;
        }
        let current_checked = match bracket[1] {
            b' ' => false,
            b'x' | b'X' => true,
            _ => return None,
        };

        let start = trimmed_start + bullet_len + space_len;
        Some(Self {
            start,
            end: start + 3,
            checked: current_checked,
        })
    }

    fn rewrite(&self, line: &str, new_checked: bool) -> String {
        let mut result = String::with_capacity(line.len());
        result.push_str(&line[..self.start]);
        result.push('[');
        result.push(if new_checked { 'x' } else { ' ' });
        result.push(']');
        result.push_str(&line[self.end..]);
        result
    }
}

pub(crate) fn rewrite_frontmatter_file(
    hq_dir: &Path,
    file: &str,
    rewrite: impl FnOnce(Vec<String>) -> Result<Vec<String>, ProjectFileError>,
) -> Result<(), ProjectFileError> {
    ProjectDocument::read(hq_dir, file)?.rewrite_frontmatter(rewrite)
}

pub(crate) fn rewrite_frontmatter_fields(
    hq_dir: &Path,
    file: &str,
    rewrite: impl FnOnce(&mut FrontmatterLines) -> Result<(), ProjectFileError>,
) -> Result<(), ProjectFileError> {
    rewrite_frontmatter_file(hq_dir, file, |lines| {
        let mut frontmatter = FrontmatterLines::new(lines);
        rewrite(&mut frontmatter)?;
        Ok(frontmatter.into_inner())
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    use std::path::Path;

    use tempfile::tempdir;

    use super::{
        project_body, read_project_body, resolve_project_path, rewrite_frontmatter_file,
        toggle_checkbox_line, write_project_body, FrontmatterLines, ProjectFileError,
    };

    #[test]
    fn project_body_ignores_dashes_inside_frontmatter_values() {
        let text = r#"---
title: "Bug repro"
status: active
priority: 40---
notes: keep this in frontmatter
---

Actual body text.
"#;

        assert_eq!(project_body(text), "Actual body text.\n");
    }

    #[test]
    fn resolve_project_path_rejects_absolute_paths() {
        let hq_dir = Path::new("/tmp/hq");
        assert!(resolve_project_path(hq_dir, "/tmp/outside.md").is_err());
    }

    #[test]
    fn resolve_project_path_rejects_parent_traversal() {
        let hq_dir = Path::new("/tmp/hq");
        assert!(resolve_project_path(hq_dir, "../outside.md").is_err());
    }

    #[test]
    fn resolve_project_path_accepts_relative_markdown_paths() {
        let hq_dir = Path::new("/tmp/hq");
        let resolved = resolve_project_path(hq_dir, "research/project.md").unwrap();
        assert_eq!(resolved, hq_dir.join("research/project.md"));
    }

    #[test]
    fn resolve_project_path_rejects_non_markdown_files_when_requested() {
        let hq_dir = Path::new("/tmp/hq");
        assert!(resolve_project_path(hq_dir, "research/notes.txt").is_err());
    }

    #[test]
    fn write_project_body_preserves_frontmatter() {
        let tmp = tempdir().unwrap();
        let hq_dir = tmp.path();
        let track_dir = hq_dir.join("research");
        fs::create_dir_all(&track_dir).unwrap();
        let file = track_dir.join("project.md");
        fs::write(
            &file,
            "---\ntitle: \"Test\"\nstatus: active\npriority: 10\n---\n\nOld body.\n",
        )
        .unwrap();

        write_project_body(hq_dir, "research/project.md", "New body.").unwrap();

        let rewritten = fs::read_to_string(&file).unwrap();
        assert!(rewritten.contains("priority: 10"));
        assert!(rewritten.ends_with("\n\nNew body.\n"));
        assert_eq!(
            read_project_body(hq_dir, "research/project.md").unwrap(),
            "New body.\n"
        );
    }

    #[test]
    fn rewrite_frontmatter_preserves_body_spacing() {
        let tmp = tempdir().unwrap();
        let hq_dir = tmp.path();
        let track_dir = hq_dir.join("research");
        fs::create_dir_all(&track_dir).unwrap();
        let file = track_dir.join("project.md");
        fs::write(
            &file,
            "---\ntitle: \"Test\"\nstatus: active\n---\n\nBody text.\n",
        )
        .unwrap();

        rewrite_frontmatter_file(hq_dir, "research/project.md", |mut lines| {
            lines.push("priority: 20".to_string());
            Ok(lines)
        })
        .unwrap();

        let rewritten = fs::read_to_string(&file).unwrap();
        assert!(rewritten.contains("priority: 20"));
        assert!(rewritten.ends_with("---\n\nBody text.\n"));
    }

    #[test]
    fn write_project_body_preserves_trailing_spaces_in_body() {
        let tmp = tempdir().unwrap();
        let hq_dir = tmp.path();
        let track_dir = hq_dir.join("research");
        fs::create_dir_all(&track_dir).unwrap();
        let file = track_dir.join("project.md");
        fs::write(
            &file,
            "---\ntitle: \"Test\"\nstatus: active\n---\n\nOld body.\n",
        )
        .unwrap();

        write_project_body(hq_dir, "research/project.md", "Keep these spaces  ").unwrap();

        assert_eq!(
            read_project_body(hq_dir, "research/project.md").unwrap(),
            "Keep these spaces  \n"
        );
    }

    #[test]
    fn write_project_body_preserves_trailing_blank_lines() {
        let tmp = tempdir().unwrap();
        let hq_dir = tmp.path();
        let track_dir = hq_dir.join("research");
        fs::create_dir_all(&track_dir).unwrap();
        let file = track_dir.join("project.md");
        fs::write(
            &file,
            "---\ntitle: \"Test\"\nstatus: active\n---\n\nOld body.\n",
        )
        .unwrap();

        write_project_body(hq_dir, "research/project.md", "Line 1\n\n").unwrap();

        assert_eq!(
            read_project_body(hq_dir, "research/project.md").unwrap(),
            "Line 1\n\n"
        );

        let rewritten = fs::read_to_string(&file).unwrap();
        assert!(rewritten.ends_with("\n\nLine 1\n\n"));
    }

    #[test]
    fn read_project_body_preserves_leading_indentation() {
        let tmp = tempdir().unwrap();
        let hq_dir = tmp.path();
        let track_dir = hq_dir.join("research");
        fs::create_dir_all(&track_dir).unwrap();
        fs::write(
            track_dir.join("project.md"),
            "---\ntitle: \"Test\"\nstatus: active\n---\n\n    let x = 1;\n",
        )
        .unwrap();

        assert_eq!(
            read_project_body(hq_dir, "research/project.md").unwrap(),
            "    let x = 1;\n"
        );
    }

    #[test]
    fn write_project_body_requires_frontmatter() {
        let tmp = tempdir().unwrap();
        let hq_dir = tmp.path();
        let track_dir = hq_dir.join("research");
        fs::create_dir_all(&track_dir).unwrap();
        fs::write(track_dir.join("project.md"), "no frontmatter").unwrap();

        let error = write_project_body(hq_dir, "research/project.md", "Body").unwrap_err();
        assert!(matches!(error, ProjectFileError::Frontmatter { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn read_project_body_rejects_symlinked_paths_outside_hq_dir() {
        let tmp = tempdir().unwrap();
        let hq_dir = tmp.path().join("hq");
        let track_dir = hq_dir.join("research");
        fs::create_dir_all(&track_dir).unwrap();

        let outside = tmp.path().join("outside.md");
        fs::write(
            &outside,
            "---\ntitle: \"Outside\"\nstatus: active\n---\n\nSecret notes.\n",
        )
        .unwrap();
        symlink(&outside, track_dir.join("linked.md")).unwrap();

        let error = read_project_body(&hq_dir, "research/linked.md").unwrap_err();
        assert!(matches!(error, ProjectFileError::InvalidPath(_)));
    }

    #[test]
    fn write_errors_include_the_target_file() {
        let error = ProjectFileError::Write {
            file: "research/project.md".to_string(),
            source: io::Error::new(io::ErrorKind::PermissionDenied, "blocked"),
        };

        assert_eq!(error.to_string(), "research/project.md: blocked");
    }

    #[test]
    fn toggle_checkbox_line_preserves_bullet_spacing_and_text() {
        assert_eq!(
            toggle_checkbox_line("  - [ ] Draft section", false, true),
            Some("  - [x] Draft section".to_string())
        );
        assert_eq!(
            toggle_checkbox_line("\t* [X] Review", true, false),
            Some("\t* [ ] Review".to_string())
        );
    }

    #[test]
    fn toggle_checkbox_line_rejects_mismatched_or_non_checkbox_lines() {
        assert_eq!(toggle_checkbox_line("- [ ] Draft", true, true), None);
        assert_eq!(toggle_checkbox_line("- Draft", false, true), None);
        assert_eq!(toggle_checkbox_line("-[ ] Draft", false, true), None);
        assert_eq!(toggle_checkbox_line("1. [ ] Draft", false, true), None);
    }

    #[test]
    fn frontmatter_replace_updates_all_matching_fields() {
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
    fn frontmatter_upsert_after_appends_when_anchor_is_missing() {
        let mut frontmatter = FrontmatterLines::new(vec!["title: Project".to_string()]);

        frontmatter.upsert_after("priority", 70, "status");

        assert_eq!(
            frontmatter.into_inner(),
            vec!["title: Project", "priority: 70"]
        );
    }
}
