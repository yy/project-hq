use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::frontmatter::split_frontmatter;

#[derive(Debug)]
pub enum ProjectFileError {
    InvalidPath(String),
    Read { file: String, source: io::Error },
    Write(io::Error),
    Frontmatter { file: String, reason: &'static str },
    MissingField { file: String, field: &'static str },
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
}

impl fmt::Display for ProjectFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(file) => write!(f, "Invalid file path: {file}"),
            Self::Read { file, source } => write!(f, "{file}: {source}"),
            Self::Write(source) => write!(f, "Write failed: {source}"),
            Self::Frontmatter { file, reason } => write!(f, "{reason} in {file}"),
            Self::MissingField { file, field } => write!(f, "No {field} field in {file}"),
        }
    }
}

impl std::error::Error for ProjectFileError {}

fn resolve_project_path(
    hq_dir: &Path,
    file: &str,
    markdown_only: bool,
) -> Result<PathBuf, ProjectFileError> {
    let path = Path::new(file);
    if (markdown_only && !file.ends_with(".md"))
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

    Ok(hq_dir.join(path))
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
    if text.starts_with("---") {
        split_frontmatter(text)
            .map(|(_, body)| strip_frontmatter_separators(body))
            .unwrap_or(text)
    } else {
        text
    }
}

pub fn read_project_body(hq_dir: &Path, file: &str) -> Result<String, ProjectFileError> {
    let filepath = resolve_project_path(hq_dir, file, true)?;
    let text = fs::read_to_string(&filepath).map_err(|source| ProjectFileError::Read {
        file: file.to_string(),
        source,
    })?;
    Ok(project_body(&text).to_string())
}

pub fn write_project_body(hq_dir: &Path, file: &str, body: &str) -> Result<(), ProjectFileError> {
    let filepath = resolve_project_path(hq_dir, file, true)?;
    let text = fs::read_to_string(&filepath).map_err(|source| ProjectFileError::Read {
        file: file.to_string(),
        source,
    })?;
    let (fm_text, _) =
        split_frontmatter(&text).map_err(|reason| ProjectFileError::Frontmatter {
            file: file.to_string(),
            reason,
        })?;

    let new_body = body.trim_end();
    let result = if new_body.is_empty() {
        format!("---{fm_text}---\n")
    } else {
        format!("---{fm_text}---\n\n{new_body}\n")
    };

    fs::write(&filepath, result).map_err(ProjectFileError::Write)
}

pub(crate) fn rewrite_frontmatter_file(
    hq_dir: &Path,
    file: &str,
    rewrite: impl FnOnce(Vec<String>) -> Result<Vec<String>, ProjectFileError>,
) -> Result<(), ProjectFileError> {
    let filepath = resolve_project_path(hq_dir, file, true)?;
    let text = fs::read_to_string(&filepath).map_err(|source| ProjectFileError::Read {
        file: file.to_string(),
        source,
    })?;
    let (fm_text, body) =
        split_frontmatter(&text).map_err(|reason| ProjectFileError::Frontmatter {
            file: file.to_string(),
            reason,
        })?;

    let lines = fm_text.lines().map(str::to_string).collect();
    let new_fm = rewrite(lines)?.join("\n");
    let result = format!("---{new_fm}\n---{body}");
    fs::write(&filepath, result).map_err(ProjectFileError::Write)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::tempdir;

    use super::{
        project_body, read_project_body, resolve_project_path, write_project_body, ProjectFileError,
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
        assert!(resolve_project_path(hq_dir, "/tmp/outside.md", true).is_err());
    }

    #[test]
    fn resolve_project_path_rejects_parent_traversal() {
        let hq_dir = Path::new("/tmp/hq");
        assert!(resolve_project_path(hq_dir, "../outside.md", true).is_err());
    }

    #[test]
    fn resolve_project_path_accepts_relative_markdown_paths() {
        let hq_dir = Path::new("/tmp/hq");
        let resolved = resolve_project_path(hq_dir, "research/project.md", true).unwrap();
        assert_eq!(resolved, hq_dir.join("research/project.md"));
    }

    #[test]
    fn resolve_project_path_rejects_non_markdown_files_when_requested() {
        let hq_dir = Path::new("/tmp/hq");
        assert!(resolve_project_path(hq_dir, "research/notes.txt", true).is_err());
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

        write_project_body(hq_dir, "research/project.md", "New body.\n\n").unwrap();

        let rewritten = fs::read_to_string(&file).unwrap();
        assert!(rewritten.contains("priority: 10"));
        assert!(rewritten.ends_with("\n\nNew body.\n"));
        assert_eq!(
            read_project_body(hq_dir, "research/project.md").unwrap(),
            "New body.\n"
        );
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
}
