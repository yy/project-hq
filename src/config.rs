use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::frontmatter::parse_frontmatter;

pub const DEFAULT_STATUSES: &[&str] = &[
    "active",
    "waiting",
    "deferred",
    "submitted",
    "done",
    "dropped",
];
pub const DEFAULT_STALE_DAYS: i64 = 30;

#[derive(Debug, Deserialize)]
struct ConfigFile {
    tracks: Option<Vec<String>>,
    skip_files: Option<Vec<String>>,
    stale_days: Option<i64>,
    statuses: Option<Vec<String>>,
}

#[derive(Debug)]
pub struct Config {
    pub tracks: Vec<String>,
    pub skip_files: Vec<String>,
    pub stale_days: i64,
    pub statuses: Vec<String>,
}

impl Config {
    /// Load config from `hq.toml` in the data directory.
    /// Falls back to auto-discovering tracks from subdirectories.
    pub fn load(hq_dir: &Path) -> Self {
        let config_path = hq_dir.join("hq.toml");

        if let Ok(text) = fs::read_to_string(&config_path) {
            if let Ok(cf) = toml::from_str::<ConfigFile>(&text) {
                return Self {
                    tracks: cf.tracks.unwrap_or_else(|| Self::discover_tracks(hq_dir)),
                    skip_files: cf.skip_files.unwrap_or_default(),
                    stale_days: cf.stale_days.unwrap_or(DEFAULT_STALE_DAYS),
                    statuses: cf.statuses.unwrap_or_else(default_statuses),
                };
            }
        }

        // No config file — auto-discover
        Self {
            tracks: Self::discover_tracks(hq_dir),
            skip_files: vec![],
            stale_days: DEFAULT_STALE_DAYS,
            statuses: default_statuses(),
        }
    }

    /// Auto-discover tracks by finding subdirectories that contain .md files
    /// with YAML frontmatter (start with "---").
    fn discover_tracks(hq_dir: &Path) -> Vec<String> {
        let skip_dirs = ["scripts", "web", "cli", ".git", "node_modules", "specs"];

        let mut tracks: Vec<String> = fs::read_dir(hq_dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.')
                    || name.starts_with('_')
                    || skip_dirs.contains(&name.as_str())
                {
                    return None;
                }
                // Check if dir contains at least one .md file with frontmatter
                let has_projects = fs::read_dir(e.path())
                    .into_iter()
                    .flatten()
                    .filter_map(|f| f.ok())
                    .any(|f| {
                        let fname = f.file_name();
                        let fname = fname.to_string_lossy();
                        if !fname.ends_with(".md") {
                            return false;
                        }
                        fs::read_to_string(f.path())
                            .map(|text| parse_frontmatter(&text).is_some())
                            .unwrap_or(false)
                    });
                if has_projects {
                    Some(name)
                } else {
                    None
                }
            })
            .collect();

        tracks.sort();
        tracks
    }
}

fn default_statuses() -> Vec<String> {
    DEFAULT_STATUSES.iter().map(|s| s.to_string()).collect()
}
