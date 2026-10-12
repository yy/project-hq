use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use chrono::{Local, NaiveDate};

use crate::action::{parse_actions, ActionMode, ActionSource};
use crate::config::Config;
use crate::frontmatter::parse_frontmatter;
use crate::project::{valid_deferred_until, DEFAULT_PRIORITY};
use crate::project_file::{
    project_body, read_project_text, resolve_project_path, rewrite_frontmatter_fields,
    validate_project_file_for_rewrite, ProjectFileError,
};

const ARCHIVE_DIR: &str = "archive";
const DONE_ON: &str = "done_on";

/// Terminal statuses whose projects belong in `<track>/archive/`.
pub fn is_archived_status(status: &str) -> bool {
    matches!(status, "done" | "dropped")
}

pub struct MoveOptions {
    pub file: String,
    pub to_status: String,
    pub priority: Option<f64>,
    pub waiting_on: Option<String>,
}

pub struct MetadataOptions {
    pub file: String,
    pub title: String,
    pub status: String,
    pub priority: f64,
    pub owner: String,
    pub my_next: String,
    pub waiting_on: String,
    pub waiting_since: String,
    pub deadline: String,
    pub deferred_until: String,
    pub action_mode: String,
}

fn validate_status(file: &str, status: &str) -> Result<(), ProjectFileError> {
    if status.trim().is_empty() {
        return Err(ProjectFileError::InvalidStatus {
            file: file.to_string(),
        });
    }

    Ok(())
}

fn is_default_priority(priority: f64) -> bool {
    (priority - DEFAULT_PRIORITY).abs() < f64::EPSILON
}

pub fn move_project(hq_dir: &Path, opts: &MoveOptions) -> Result<(), ProjectFileError> {
    validate_status(&opts.file, &opts.to_status)?;

    let text = read_project_text(hq_dir, &opts.file)?;
    let fields = parse_frontmatter(&text).ok_or_else(|| ProjectFileError::Frontmatter {
        file: opts.file.clone(),
        reason: "Invalid frontmatter or missing title/status",
    })?;
    let current_status = fields
        .get("status")
        .ok_or_else(|| ProjectFileError::missing_field(&opts.file, "status"))?;
    let entering_done = !is_archived_status(current_status) && is_archived_status(&opts.to_status);
    let leaving_done = is_archived_status(current_status) && !is_archived_status(&opts.to_status);
    let entering_waiting = current_status != "waiting" && opts.to_status == "waiting";
    let leaving_waiting = current_status == "waiting" && opts.to_status != "waiting";

    let waiting_on = if entering_waiting {
        let action_mode = ActionMode::from_field(fields.get("action_mode").map(String::as_str));
        let people: BTreeSet<String> = parse_actions(project_body(&text), None, action_mode, true)
            .into_iter()
            .filter(|action| {
                action.source == ActionSource::Checklist && !action.completed && action.available
            })
            .flat_map(|action| action.people)
            .collect();

        if people.len() == 1 {
            people.iter().next().cloned()
        } else {
            Some(
                opts.waiting_on
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .ok_or_else(|| ProjectFileError::WaitingOnRequired {
                        file: opts.file.clone(),
                        people: people.into_iter().collect(),
                    })?,
            )
        }
    } else {
        None
    };

    rewrite_frontmatter_fields(hq_dir, &opts.file, |frontmatter| {
        if !frontmatter.replace("status", &opts.to_status) {
            return Err(ProjectFileError::missing_field(&opts.file, "status"));
        }

        if let Some(p) = opts.priority {
            if is_default_priority(p) {
                frontmatter.replace("priority", p);
            } else {
                frontmatter.upsert_after("priority", p, "status");
            }
        }

        if let Some(waiting_on) = waiting_on.as_deref() {
            frontmatter.upsert_string_after("waiting_on", waiting_on, "status");
            frontmatter.upsert_after(
                "waiting_since",
                Local::now().date_naive().format("%Y-%m-%d"),
                "waiting_on",
            );
        } else if leaving_waiting {
            frontmatter.remove("waiting_on");
            frontmatter.remove("waiting_since");
        }

        if entering_done {
            frontmatter.upsert_after(DONE_ON, today(), "status");
        } else if leaving_done {
            frontmatter.remove(DONE_ON);
        }

        Ok(())
    })
}

fn today() -> impl std::fmt::Display {
    Local::now().date_naive().format("%Y-%m-%d")
}

#[derive(Debug, Default)]
pub struct ArchiveSweep {
    pub archived: Vec<String>,
    pub stamped: Vec<String>,
}

/// Archive done/dropped projects whose `done_on` is at least `days` old.
/// Projects without `done_on` get stamped with `today`, starting their clock.
pub fn archive_done_projects(
    hq_dir: &Path,
    config: &Config,
    today: NaiveDate,
    days: i64,
) -> Result<ArchiveSweep, ProjectFileError> {
    let mut sweep = ArchiveSweep::default();
    for track in &config.tracks {
        for path in crate::sorted_markdown_files(&hq_dir.join(track), &config.skip_files) {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let file = format!("{track}/{name}");
            let Some(fields) = fs::read_to_string(&path)
                .ok()
                .and_then(|text| parse_frontmatter(&text))
            else {
                continue;
            };
            if !fields.get("status").is_some_and(|s| is_archived_status(s)) {
                continue;
            }
            match fields
                .get(DONE_ON)
                .and_then(|d| NaiveDate::parse_from_str(d.trim_matches('"'), "%Y-%m-%d").ok())
            {
                Some(done_on) if (today - done_on).num_days() >= days => {
                    if let Some(target) = archive_project(hq_dir, &file, today)? {
                        sweep.archived.push(target);
                    }
                }
                Some(_) => {}
                None => {
                    rewrite_frontmatter_fields(hq_dir, &file, |frontmatter| {
                        frontmatter.upsert_after(DONE_ON, today.format("%Y-%m-%d"), "status");
                        Ok(())
                    })?;
                    sweep.stamped.push(file);
                }
            }
        }
    }
    Ok(sweep)
}

/// Move `<track>/<name>.md` to `<track>/archive/<date>-<name>.md`.
/// Returns the new path, or `None` when the file is already in an archive.
pub fn archive_project(
    hq_dir: &Path,
    file: &str,
    date: NaiveDate,
) -> Result<Option<String>, ProjectFileError> {
    let source = resolve_project_path(hq_dir, file)?;
    let rel = Path::new(file);
    let (Some(track), Some(name)) = (
        rel.parent().filter(|p| !p.as_os_str().is_empty()),
        rel.file_name().and_then(|n| n.to_str()),
    ) else {
        return Err(ProjectFileError::InvalidPath(file.to_string()));
    };
    if track.file_name().is_some_and(|dir| dir == ARCHIVE_DIR) {
        return Ok(None);
    }

    let target = track
        .join(ARCHIVE_DIR)
        .join(format!("{}-{name}", date.format("%Y-%m-%d")))
        .to_string_lossy()
        .into_owned();
    let target_path = resolve_project_path(hq_dir, &target)?;
    if target_path.exists() {
        return Err(ProjectFileError::AlreadyExists {
            kind: "Archived project",
            name: target,
        });
    }
    let write_error = |source| ProjectFileError::Write {
        file: target.clone(),
        source,
    };
    fs::create_dir_all(hq_dir.join(track).join(ARCHIVE_DIR)).map_err(write_error)?;
    fs::rename(source, &target_path).map_err(write_error)?;
    Ok(Some(target))
}

pub fn defer_project(hq_dir: &Path, file: &str, until: &str) -> Result<(), ProjectFileError> {
    if !valid_deferred_until(until) {
        return Err(ProjectFileError::InvalidDate {
            file: file.to_string(),
            field: "deferred_until",
        });
    }

    rewrite_frontmatter_fields(hq_dir, file, |frontmatter| {
        frontmatter.upsert_after("deferred_until", until, "status");
        Ok(())
    })
}

pub fn update_project_metadata(
    hq_dir: &Path,
    options: &MetadataOptions,
) -> Result<(), ProjectFileError> {
    if options.title.trim().is_empty() {
        return Err(ProjectFileError::missing_field(&options.file, "title"));
    }
    validate_status(&options.file, &options.status)?;
    if !options.priority.is_finite() {
        return Err(ProjectFileError::Frontmatter {
            file: options.file.clone(),
            reason: "Invalid priority",
        });
    }
    if !options.deferred_until.is_empty() && !valid_deferred_until(&options.deferred_until) {
        return Err(ProjectFileError::InvalidDate {
            file: options.file.clone(),
            field: "deferred_until",
        });
    }
    let action_mode =
        ActionMode::parse(&options.action_mode).ok_or_else(|| ProjectFileError::Frontmatter {
            file: options.file.clone(),
            reason: "Invalid action_mode",
        })?;

    rewrite_frontmatter_fields(hq_dir, &options.file, |frontmatter| {
        if !frontmatter.replace_string("title", &options.title) {
            return Err(ProjectFileError::missing_field(&options.file, "title"));
        }
        if !frontmatter.replace_string("status", &options.status) {
            return Err(ProjectFileError::missing_field(&options.file, "status"));
        }
        frontmatter.upsert_after("priority", options.priority, "status");

        for (field, value, anchor) in [
            ("owner", options.owner.as_str(), "title"),
            ("my_next", options.my_next.as_str(), "priority"),
            ("waiting_on", options.waiting_on.as_str(), "status"),
            (
                "waiting_since",
                options.waiting_since.as_str(),
                "waiting_on",
            ),
            ("deadline", options.deadline.as_str(), "priority"),
            ("deferred_until", options.deferred_until.as_str(), "status"),
            ("action_mode", action_mode.as_str(), "status"),
        ] {
            if value.is_empty() {
                frontmatter.remove(field);
            } else {
                frontmatter.upsert_string_after(field, value, anchor);
            }
        }
        Ok(())
    })
}

/// Set priority on a single file's frontmatter.
fn set_priority(hq_dir: &Path, file: &str, priority: f64) -> Result<(), ProjectFileError> {
    rewrite_frontmatter_fields(hq_dir, file, |frontmatter| {
        frontmatter.upsert_after("priority", priority, "status");
        Ok(())
    })
}

/// Assign descending priorities to an ordered list of files.
/// First item gets highest priority (top of board).
pub fn reorder_projects(hq_dir: &Path, files: &[String]) -> Result<(), ProjectFileError> {
    for file in files {
        validate_project_file_for_rewrite(hq_dir, file)?;
    }

    let n = files.len();
    for (i, file) in files.iter().enumerate() {
        let priority = ((n - i) * 10) as f64;
        set_priority(hq_dir, file, priority)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::NaiveDate;
    use tempfile::tempdir;

    use super::{
        archive_done_projects, archive_project, is_archived_status, move_project, MoveOptions,
    };
    use crate::config::Config;
    use crate::project_file::ProjectFileError;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 12).unwrap()
    }

    #[test]
    fn archives_into_dated_track_archive() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("service")).unwrap();
        fs::write(
            tmp.path().join("service/yy-rec.md"),
            "---\ntitle: Rec\nstatus: done\n---\n",
        )
        .unwrap();

        let target = archive_project(tmp.path(), "service/yy-rec.md", date()).unwrap();

        assert_eq!(
            target.as_deref(),
            Some("service/archive/2026-10-12-yy-rec.md")
        );
        assert!(!tmp.path().join("service/yy-rec.md").exists());
        assert!(tmp
            .path()
            .join("service/archive/2026-10-12-yy-rec.md")
            .exists());
    }

    #[test]
    fn leaves_already_archived_files_in_place() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("service/archive")).unwrap();
        fs::write(
            tmp.path().join("service/archive/2026-01-01-yy-rec.md"),
            "---\ntitle: Rec\nstatus: done\n---\n",
        )
        .unwrap();

        let target =
            archive_project(tmp.path(), "service/archive/2026-01-01-yy-rec.md", date()).unwrap();

        assert_eq!(target, None);
        assert!(tmp
            .path()
            .join("service/archive/2026-01-01-yy-rec.md")
            .exists());
    }

    #[test]
    fn refuses_to_overwrite_an_existing_archive_file() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("service/archive")).unwrap();
        fs::write(tmp.path().join("service/yy-rec.md"), "new").unwrap();
        fs::write(
            tmp.path().join("service/archive/2026-10-12-yy-rec.md"),
            "old",
        )
        .unwrap();

        let err = archive_project(tmp.path(), "service/yy-rec.md", date()).unwrap_err();

        assert!(matches!(err, ProjectFileError::AlreadyExists { .. }));
        assert!(tmp.path().join("service/yy-rec.md").exists());
    }

    fn write(root: &std::path::Path, file: &str, text: &str) {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn move_to(root: &std::path::Path, file: &str, status: &str) {
        move_project(
            root,
            &MoveOptions {
                file: file.into(),
                to_status: status.into(),
                priority: None,
                waiting_on: Some("x".into()),
            },
        )
        .unwrap();
    }

    #[test]
    fn moving_to_done_stamps_done_on_and_reopening_clears_it() {
        let tmp = tempdir().unwrap();
        write(
            tmp.path(),
            "lab/a.md",
            "---\ntitle: A\nstatus: active\n---\n",
        );

        move_to(tmp.path(), "lab/a.md", "done");
        let text = fs::read_to_string(tmp.path().join("lab/a.md")).unwrap();
        assert!(text.contains("done_on: "), "{text}");
        assert!(tmp.path().join("lab/a.md").exists());

        move_to(tmp.path(), "lab/a.md", "active");
        let text = fs::read_to_string(tmp.path().join("lab/a.md")).unwrap();
        assert!(!text.contains("done_on"), "{text}");
    }

    #[test]
    fn sweep_archives_old_done_stamps_unstamped_and_keeps_recent() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "lab/old.md",
            "---\ntitle: O\nstatus: done\ndone_on: 2026-09-01\n---\n",
        );
        write(
            root,
            "lab/new.md",
            "---\ntitle: N\nstatus: dropped\ndone_on: 2026-10-10\n---\n",
        );
        write(root, "lab/bare.md", "---\ntitle: B\nstatus: done\n---\n");
        write(root, "lab/live.md", "---\ntitle: L\nstatus: active\n---\n");
        let config = Config {
            tracks: vec!["lab".into()],
            skip_files: Vec::new(),
            stale_days: 30,
            statuses: Vec::new(),
            default_owner: None,
            pulse_tracks: Vec::new(),
        };

        let sweep = archive_done_projects(root, &config, date(), 14).unwrap();

        assert_eq!(sweep.archived, vec!["lab/archive/2026-10-12-old.md"]);
        assert_eq!(sweep.stamped, vec!["lab/bare.md"]);
        assert!(root.join("lab/new.md").exists());
        assert!(root.join("lab/live.md").exists());
        assert!(fs::read_to_string(root.join("lab/bare.md"))
            .unwrap()
            .contains("done_on: 2026-10-12"));
    }

    #[test]
    fn only_terminal_statuses_archive() {
        assert!(is_archived_status("done"));
        assert!(is_archived_status("dropped"));
        assert!(!is_archived_status("waiting"));
    }
}
