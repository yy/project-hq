use std::fs;
use std::path::{Component, Path};

use serde::Deserialize;

use crate::{sorted_dir_entries, track_contains_projects};

pub const DEFAULT_STATUSES: &[&str] = &[
    "my-plate",
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
        fs::read_to_string(&config_path)
            .ok()
            .and_then(|text| toml::from_str::<ConfigFile>(&text).ok())
            .map_or_else(
                || Self::with_defaults(hq_dir),
                |config| Self::from_file(hq_dir, config),
            )
    }

    fn from_file(hq_dir: &Path, config: ConfigFile) -> Self {
        let skip_tracks = config.skip_tracks.unwrap_or_else(default_skip_tracks);

        Self {
            tracks: config
                .tracks
                .map(|tracks| {
                    tracks
                        .into_iter()
                        .filter(|track| is_valid_track(hq_dir, track))
                        .collect()
                })
                .unwrap_or_else(|| Self::discover_tracks(hq_dir, &skip_tracks)),
            skip_files: config.skip_files.unwrap_or_default(),
            stale_days: config.stale_days.unwrap_or(DEFAULT_STALE_DAYS),
            statuses: config.statuses.unwrap_or_else(default_statuses),
        }
    }

    fn with_defaults(hq_dir: &Path) -> Self {
        let skip_tracks = default_skip_tracks();

        Self {
            tracks: Self::discover_tracks(hq_dir, &skip_tracks),
            skip_files: Vec::new(),
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

fn is_valid_track(hq_dir: &Path, track: &str) -> bool {
    let path = Path::new(track);
    let lexically_safe = !track.is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));

    if !lexically_safe {
        return false;
    }

    let track_path = hq_dir.join(path);
    match (fs::canonicalize(hq_dir), fs::canonicalize(&track_path)) {
        (Ok(canonical_hq_dir), Ok(canonical_track_path)) => {
            canonical_track_path.starts_with(canonical_hq_dir)
        }
        _ => true,
    }
}

fn default_statuses() -> Vec<String> {
    DEFAULT_STATUSES.iter().map(|s| s.to_string()).collect()
}

fn default_skip_tracks() -> Vec<String> {
    DEFAULT_SKIP_TRACKS.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{Config, DEFAULT_STALE_DAYS};

    fn write_project(base: &std::path::Path, track: &str, filename: &str) {
        let dir = base.join(track);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(filename),
            "---\ntitle: \"Project\"\nstatus: active\n---\n",
        )
        .unwrap();
    }

    #[test]
    fn default_statuses_include_my_plate_first() {
        let temp = tempdir().unwrap();
        let hq_dir = temp.path();
        write_project(hq_dir, "research", "project.md");

        let config = Config::load(hq_dir);

        assert_eq!(config.statuses[0], "my-plate");
        assert!(config.statuses.contains(&"active".to_string()));
    }

    #[test]
    fn invalid_toml_falls_back_to_defaults() {
        let temp = tempdir().unwrap();
        let hq_dir = temp.path();
        write_project(hq_dir, "research", "project.md");
        fs::write(hq_dir.join("hq.toml"), "tracks = [research").unwrap();

        let config = Config::load(hq_dir);

        assert_eq!(config.tracks, vec!["research"]);
        assert!(config.skip_files.is_empty());
        assert_eq!(config.stale_days, DEFAULT_STALE_DAYS);
        assert_eq!(config.statuses[0], "my-plate");
    }
}
