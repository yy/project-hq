pub mod commands;
pub mod config;
pub mod frontmatter;
pub mod mover;
pub mod project;
pub mod project_file;
pub mod web;

use std::fs;
use std::path::{Path, PathBuf};

use config::Config;
use frontmatter::parse_frontmatter;
use project::Project;

pub(crate) fn sorted_markdown_files(dir: &Path, skip_files: &[String]) -> Vec<PathBuf> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            (name.ends_with(".md") && !skip_files.iter().any(|skip| skip == name.as_ref()))
                .then(|| entry.path())
        })
        .collect();
    entries.sort();
    entries
}

pub(crate) fn track_contains_projects(track_path: &Path) -> bool {
    sorted_markdown_files(track_path, &[])
        .into_iter()
        .any(|path| {
            fs::read_to_string(path)
                .map(|text| parse_frontmatter(&text).is_some())
                .unwrap_or(false)
        })
}

pub fn load_all(hq_dir: &Path, config: &Config) -> Vec<Project> {
    let mut projects = Vec::new();
    for track in &config.tracks {
        let track_path = hq_dir.join(track);
        if !track_path.is_dir() {
            continue;
        }

        for path in sorted_markdown_files(&track_path, &config.skip_files) {
            if let Some(p) = Project::from_file(&path, track, hq_dir) {
                projects.push(p);
            }
        }
    }
    projects
}
