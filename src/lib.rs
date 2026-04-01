pub mod config;
pub mod frontmatter;
pub mod mover;
pub mod project;
pub mod project_file;
pub mod web;

use std::fs;
use std::path::Path;

use config::Config;
use project::Project;

pub fn load_all(hq_dir: &Path, config: &Config) -> Vec<Project> {
    let mut projects = Vec::new();
    for track in &config.tracks {
        let track_path = hq_dir.join(track);
        if !track_path.is_dir() {
            continue;
        }
        let mut entries: Vec<_> = fs::read_dir(&track_path)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                name.ends_with(".md") && !config.skip_files.contains(&name.to_string())
            })
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let path = entry.path();
            if let Some(p) = Project::from_file(&path, track, hq_dir) {
                projects.push(p);
            }
        }
    }
    projects
}
