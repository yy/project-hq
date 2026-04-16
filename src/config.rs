use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::{sorted_dir_entries, track_contains_projects};

pub const DEFAULT_STATUSES: &[&str] = &[
    "active",
    "waiting",
    "deferred",
    "submitted",
    "done",
    "dropped",
];
pub const DEFAULT_STALE_DAYS: i64 = 30;
pub const DEFAULT_SKIP_TRACKS: &[&str] =
    &["node_modules", "target", "vendor", "dist", "build", "out"];

#[derive(Debug, Deserialize)]
struct ConfigFile {
    tracks: Option<Vec<String>>,
    skip_tracks: Option<Vec<String>>,
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
                let skip_tracks = cf.skip_tracks.unwrap_or_else(default_skip_tracks);
                return Self {
                    tracks: cf
                        .tracks
                        .unwrap_or_else(|| Self::discover_tracks(hq_dir, &skip_tracks)),
                    skip_files: cf.skip_files.unwrap_or_default(),
                    stale_days: cf.stale_days.unwrap_or(DEFAULT_STALE_DAYS),
                    statuses: cf.statuses.unwrap_or_else(default_statuses),
                };
            }
        }

        // No config file — auto-discover with defaults
        Self {
            tracks: Self::discover_tracks(hq_dir, &default_skip_tracks()),
            skip_files: vec![],
            stale_days: DEFAULT_STALE_DAYS,
            statuses: default_statuses(),
        }
    }

    /// Auto-discover tracks by finding subdirectories that contain .md files
    /// with YAML frontmatter (start with "---").
    fn discover_tracks(hq_dir: &Path, skip_tracks: &[String]) -> Vec<String> {
        sorted_dir_entries(hq_dir)
            .into_iter()
            .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || name.starts_with('_') || skip_tracks.contains(&name) {
                    return None;
                }
                if track_contains_projects(&e.path()) {
                    Some(name)
                } else {
                    None
                }
            })
            .collect()
    }
}

fn default_statuses() -> Vec<String> {
    DEFAULT_STATUSES.iter().map(|s| s.to_string()).collect()
}

fn default_skip_tracks() -> Vec<String> {
    DEFAULT_SKIP_TRACKS.iter().map(|s| s.to_string()).collect()
}
