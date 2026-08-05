use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Stdio;

use chrono::{Local, NaiveDate};

use crate::config::Config;
use crate::load_all;
use crate::project::Project;

const LOAD_STATUSES: &[&str] = &["my-plate", "active"];
const ANALYSIS_STATUSES: &[&str] = &["my-plate", "active", "waiting"];
const OUTFLOW_STATUSES: &[&str] = &["submitted", "done", "dropped"];

#[derive(Debug, serde::Serialize)]
pub struct TimelineResponse {
    pub source: TimelineSource,
    pub stale_days: i64,
    pub snapshots: Vec<TimelineSnapshot>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineSource {
    GitHistory,
    CurrentOnly,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TimelineSnapshot {
    pub date: NaiveDate,
    pub projects: Vec<TimelineProject>,
    pub outflow: Vec<TimelineOutflow>,
    pub pipeline: Vec<TimelinePipeline>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TimelineProject {
    pub file: String,
    pub title: String,
    pub track: String,
    pub status: String,
    pub waiting_since: Option<NaiveDate>,
    pub deadline: Option<String>,
    pub age_days: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TimelineOutflow {
    pub track: String,
    pub status: String,
    pub count: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TimelinePipeline {
    pub track: String,
    pub status: String,
    pub count: usize,
}

#[derive(Clone)]
struct GitCommit {
    hash: String,
    date: NaiveDate,
}

pub fn build_timeline(hq_dir: &Path, config: &Config) -> TimelineResponse {
    let current_projects = load_all(hq_dir, config);
    let Some(history) = git_history(hq_dir, config).filter(|history| !history.is_empty()) else {
        return TimelineResponse {
            source: TimelineSource::CurrentOnly,
            stale_days: config.stale_days,
            snapshots: vec![current_snapshot(&current_projects)],
        };
    };

    let mut snapshots = replay_history(hq_dir, config, &history);

    push_or_replace_daily_snapshot(&mut snapshots, current_snapshot(&current_projects));
    snapshots = fill_daily_snapshots(snapshots);
    apply_project_ages(&mut snapshots);

    TimelineResponse {
        source: TimelineSource::GitHistory,
        stale_days: config.stale_days,
        snapshots,
    }
}

fn current_snapshot(projects: &[Project]) -> TimelineSnapshot {
    let date = Local::now().date_naive();
    TimelineSnapshot {
        date,
        projects: timeline_projects(projects, date),
        outflow: Vec::new(),
        pipeline: timeline_pipeline(projects),
    }
}

fn timeline_projects(projects: &[Project], date: NaiveDate) -> Vec<TimelineProject> {
    let mut projects: Vec<_> = projects
        .iter()
        .filter(|project| {
            is_analysis_status(&project.status)
                && (!is_load_status(&project.status) || project.is_visible_on(date))
        })
        .map(|project| TimelineProject {
            file: project.file.clone(),
            title: project.title.clone(),
            track: project.track.clone(),
            status: project.status.clone(),
            waiting_since: project.waiting_since,
            deadline: project.deadline.clone(),
            age_days: None,
        })
        .collect();
    projects.sort_by(|a, b| {
        a.status
            .cmp(&b.status)
            .then_with(|| a.track.cmp(&b.track))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.file.cmp(&b.file))
    });
    projects
}

fn git_history(hq_dir: &Path, config: &Config) -> Option<Vec<(GitCommit, Vec<GitChange>)>> {
    if config.tracks.is_empty() {
        return Some(Vec::new());
    }

    let mut command = Command::new("git");
    command.arg("-C").arg(hq_dir).args([
        "log",
        "--reverse",
        "--date=short",
        "--format=commit%x09%H%x09%cs",
        "--name-status",
        "-M",
        "--",
    ]);
    for track in &config.tracks {
        command.arg(track);
    }

    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let mut history = Vec::new();
    let mut current: Option<(GitCommit, Vec<GitChange>)> = None;

    for line in stdout.lines() {
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("commit\t") {
            if let Some(entry) = current.take() {
                history.push(entry);
            }
            let mut parts = rest.split('\t');
            let hash = parts.next()?.to_string();
            let date = parts.next()?;
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
            current = Some((GitCommit { hash, date }, Vec::new()));
            continue;
        }
        if let Some((_, changes)) = &mut current {
            if let Some(change) = parse_git_change(config, line) {
                changes.push(change);
            }
        }
    }
    if let Some(entry) = current {
        history.push(entry);
    }

    Some(history)
}

fn replay_history(
    hq_dir: &Path,
    config: &Config,
    history: &[(GitCommit, Vec<GitChange>)],
) -> Vec<TimelineSnapshot> {
    let mut projects_at_revisions = git_projects_at_revisions(hq_dir, config, history);
    let mut state: HashMap<String, Project> = HashMap::new();
    let mut snapshots = Vec::new();

    for (commit, changes) in history {
        let mut touched = false;
        let mut outflow_counts: HashMap<(String, String), usize> = HashMap::new();

        for change in changes {
            if let Some(old_file) = &change.old_file {
                touched = state.remove(old_file.as_str()).is_some() || touched;
            }
            if let Some(new_file) = &change.new_file {
                touched = true;
                let key = (commit.hash.clone(), new_file.clone());
                if let Some(project) = projects_at_revisions.remove(&key).flatten() {
                    if counts_as_outflow(state.get(new_file.as_str()), &project) {
                        *outflow_counts
                            .entry((project.track.clone(), project.status.clone()))
                            .or_default() += 1;
                    }
                    state.insert(new_file.clone(), project);
                } else {
                    state.remove(new_file.as_str());
                }
            }
        }

        if touched {
            push_or_replace_daily_snapshot(
                &mut snapshots,
                TimelineSnapshot {
                    date: commit.date,
                    projects: timeline_projects_from_state(&state, commit.date),
                    outflow: timeline_outflow(outflow_counts),
                    pipeline: timeline_pipeline_from_state(&state),
                },
            );
        }
    }

    snapshots
}

type RevisionProjectMap = HashMap<(String, String), Option<Project>>;

fn git_projects_at_revisions(
    hq_dir: &Path,
    config: &Config,
    history: &[(GitCommit, Vec<GitChange>)],
) -> RevisionProjectMap {
    let mut requests = Vec::new();
    for (commit, changes) in history {
        for change in changes {
            if let Some(file) = &change.new_file {
                let key = (commit.hash.clone(), file.clone());
                if !requests.contains(&key) {
                    requests.push(key);
                }
            }
        }
    }

    let Some(blobs) = git_blob_batch(hq_dir, &requests) else {
        return RevisionProjectMap::new();
    };

    requests
        .into_iter()
        .zip(blobs)
        .map(|((commit, file), blob)| {
            let project = blob.and_then(|text| {
                let track = track_for_file(config, &file)?;
                Project::from_text(&text, &track, &file)
            });
            ((commit, file), project)
        })
        .collect()
}

fn git_blob_batch(hq_dir: &Path, requests: &[(String, String)]) -> Option<Vec<Option<String>>> {
    if requests.is_empty() {
        return Some(Vec::new());
    }

    let mut child = Command::new("git")
        .arg("-C")
        .arg(hq_dir)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;

    {
        let stdin = child.stdin.as_mut()?;
        for (commit, file) in requests {
            writeln!(stdin, "{commit}:{file}").ok()?;
        }
    }

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }

    parse_cat_file_batch(&output.stdout, requests.len())
}

fn parse_cat_file_batch(output: &[u8], expected: usize) -> Option<Vec<Option<String>>> {
    let mut cursor = 0;
    let mut blobs = Vec::with_capacity(expected);

    while blobs.len() < expected {
        let header_end = output[cursor..].iter().position(|byte| *byte == b'\n')? + cursor;
        let header = std::str::from_utf8(&output[cursor..header_end]).ok()?;
        cursor = header_end + 1;

        if header.ends_with(" missing") {
            blobs.push(None);
            continue;
        }

        let mut parts = header.rsplitn(3, ' ');
        let size: usize = parts.next()?.parse().ok()?;
        let object_type = parts.next()?;
        if object_type != "blob" {
            blobs.push(None);
            continue;
        }

        let content_end = cursor.checked_add(size)?;
        let content = output.get(cursor..content_end)?;
        cursor = content_end;
        if output.get(cursor) == Some(&b'\n') {
            cursor += 1;
        }
        blobs.push(String::from_utf8(content.to_vec()).ok());
    }

    Some(blobs)
}

#[derive(Debug, Clone)]
struct GitChange {
    old_file: Option<String>,
    new_file: Option<String>,
}

fn parse_git_change(config: &Config, line: &str) -> Option<GitChange> {
    let mut parts = line.split('\t');
    let status = parts.next()?;
    match status.chars().next()? {
        'A' | 'M' => {
            let file = parts.next()?;
            project_file(config, file).map(|file| GitChange {
                old_file: None,
                new_file: Some(file),
            })
        }
        'D' => {
            let file = parts.next()?;
            project_file(config, file).map(|file| GitChange {
                old_file: Some(file),
                new_file: None,
            })
        }
        'R' => {
            let old_file = parts.next().and_then(|file| project_file(config, file));
            let new_file = parts.next().and_then(|file| project_file(config, file));
            (old_file.is_some() || new_file.is_some()).then_some(GitChange { old_file, new_file })
        }
        'C' => {
            let _old_file = parts.next();
            let new_file = parts.next()?;
            project_file(config, new_file).map(|file| GitChange {
                old_file: None,
                new_file: Some(file),
            })
        }
        _ => None,
    }
}

fn project_file(config: &Config, file: &str) -> Option<String> {
    let name = Path::new(file)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    (file.ends_with(".md")
        && !config.skip_files.iter().any(|skip| skip == name)
        && track_for_file(config, file).is_some())
    .then(|| file.to_string())
}

fn timeline_projects_from_state(
    state: &HashMap<String, Project>,
    date: NaiveDate,
) -> Vec<TimelineProject> {
    let mut projects: Vec<_> = state
        .values()
        .filter(|project| {
            is_analysis_status(&project.status)
                && (!is_load_status(&project.status) || project.is_visible_on(date))
        })
        .map(|project| TimelineProject {
            file: project.file.clone(),
            title: project.title.clone(),
            track: project.track.clone(),
            status: project.status.clone(),
            waiting_since: project.waiting_since,
            deadline: project.deadline.clone(),
            age_days: None,
        })
        .collect();
    projects.sort_by(|a, b| {
        a.status
            .cmp(&b.status)
            .then_with(|| a.track.cmp(&b.track))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.file.cmp(&b.file))
    });
    projects
}

fn timeline_pipeline(projects: &[Project]) -> Vec<TimelinePipeline> {
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for project in projects
        .iter()
        .filter(|project| project.status == "submitted")
    {
        *counts
            .entry((project.track.clone(), project.status.clone()))
            .or_default() += 1;
    }
    pipeline_from_counts(counts)
}

fn timeline_pipeline_from_state(state: &HashMap<String, Project>) -> Vec<TimelinePipeline> {
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for project in state
        .values()
        .filter(|project| project.status == "submitted")
    {
        *counts
            .entry((project.track.clone(), project.status.clone()))
            .or_default() += 1;
    }
    pipeline_from_counts(counts)
}

fn pipeline_from_counts(counts: HashMap<(String, String), usize>) -> Vec<TimelinePipeline> {
    let mut pipeline: Vec<_> = counts
        .into_iter()
        .map(|((track, status), count)| TimelinePipeline {
            track,
            status,
            count,
        })
        .collect();
    pipeline.sort_by(|a, b| a.status.cmp(&b.status).then_with(|| a.track.cmp(&b.track)));
    pipeline
}

fn is_load_status(status: &str) -> bool {
    LOAD_STATUSES.contains(&status)
}

fn is_analysis_status(status: &str) -> bool {
    ANALYSIS_STATUSES.contains(&status)
}

fn is_outflow_status(status: &str) -> bool {
    OUTFLOW_STATUSES.contains(&status)
}

fn counts_as_outflow(previous: Option<&Project>, current: &Project) -> bool {
    is_outflow_status(&current.status)
        && previous.is_some_and(|previous| previous.status != current.status)
}

fn timeline_outflow(counts: HashMap<(String, String), usize>) -> Vec<TimelineOutflow> {
    let mut outflow: Vec<_> = counts
        .into_iter()
        .map(|((track, status), count)| TimelineOutflow {
            track,
            status,
            count,
        })
        .collect();
    outflow.sort_by(|a, b| a.status.cmp(&b.status).then_with(|| a.track.cmp(&b.track)));
    outflow
}

fn push_or_replace_daily_snapshot(
    snapshots: &mut Vec<TimelineSnapshot>,
    mut snapshot: TimelineSnapshot,
) {
    if snapshots
        .last()
        .is_some_and(|last| last.date == snapshot.date)
    {
        let last_index = snapshots.len() - 1;
        snapshot.outflow = merge_outflow(&snapshots[last_index].outflow, &snapshot.outflow);
        snapshots[last_index] = snapshot;
    } else {
        snapshots.push(snapshot);
    }
}

fn merge_outflow(left: &[TimelineOutflow], right: &[TimelineOutflow]) -> Vec<TimelineOutflow> {
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for item in left.iter().chain(right) {
        *counts
            .entry((item.track.clone(), item.status.clone()))
            .or_default() += item.count;
    }
    timeline_outflow(counts)
}

fn fill_daily_snapshots(mut snapshots: Vec<TimelineSnapshot>) -> Vec<TimelineSnapshot> {
    snapshots.sort_by_key(|snapshot| snapshot.date);
    let Some(first) = snapshots.iter().position(has_timeline_data) else {
        return Vec::new();
    };
    let snapshots = &snapshots[first..];
    let first_date = snapshots[0].date;
    let last_date = snapshots
        .last()
        .map(|snapshot| snapshot.date)
        .unwrap_or(first_date);
    let mut source = snapshots.iter().peekable();
    let mut projects = Vec::new();
    let mut pipeline = Vec::new();
    let mut daily = Vec::new();
    let mut date = first_date;

    while date <= last_date {
        let outflow = if source.peek().is_some_and(|snapshot| snapshot.date == date) {
            let snapshot = source.next().expect("peeked snapshot should exist");
            projects = snapshot.projects.clone();
            pipeline = snapshot.pipeline.clone();
            snapshot.outflow.clone()
        } else {
            Vec::new()
        };
        daily.push(TimelineSnapshot {
            date,
            projects: projects.clone(),
            outflow,
            pipeline: pipeline.clone(),
        });
        date = date
            .succ_opt()
            .expect("timeline date should have a successor");
    }
    daily
}

fn has_timeline_data(snapshot: &TimelineSnapshot) -> bool {
    !snapshot.projects.is_empty() || !snapshot.outflow.is_empty() || !snapshot.pipeline.is_empty()
}

fn track_for_file(config: &Config, file: &str) -> Option<String> {
    config
        .tracks
        .iter()
        .find(|track| file == *track || file.starts_with(&format!("{track}/")))
        .cloned()
        .or_else(|| {
            PathBuf::from(file)
                .components()
                .next()
                .map(|component| component.as_os_str().to_string_lossy().to_string())
        })
}

fn apply_project_ages(snapshots: &mut [TimelineSnapshot]) {
    snapshots.sort_by_key(|snapshot| snapshot.date);
    let mut first_seen: HashMap<String, NaiveDate> = HashMap::new();

    for snapshot in snapshots {
        for project in &mut snapshot.projects {
            let seen = first_seen
                .entry(project.file.clone())
                .or_insert(snapshot.date);
            project.age_days = Some((snapshot.date - *seen).num_days().max(0));
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{
        apply_project_ages, fill_daily_snapshots, is_analysis_status, is_load_status,
        TimelineOutflow, TimelineProject, TimelineSnapshot,
    };

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn project(file: &str) -> TimelineProject {
        TimelineProject {
            file: file.to_string(),
            title: file.to_string(),
            track: "research".to_string(),
            status: "active".to_string(),
            waiting_since: None,
            deadline: None,
            age_days: None,
        }
    }

    #[test]
    fn project_age_starts_when_project_first_appears() {
        let mut snapshots = vec![
            TimelineSnapshot {
                date: date("2026-05-01"),
                projects: vec![project("research/a.md")],
                outflow: Vec::new(),
                pipeline: Vec::new(),
            },
            TimelineSnapshot {
                date: date("2026-05-03"),
                projects: vec![project("research/a.md"), project("research/b.md")],
                outflow: Vec::new(),
                pipeline: Vec::new(),
            },
        ];

        apply_project_ages(&mut snapshots);

        assert_eq!(snapshots[0].projects[0].age_days, Some(0));
        assert_eq!(snapshots[1].projects[0].age_days, Some(2));
        assert_eq!(snapshots[1].projects[1].age_days, Some(0));
    }

    #[test]
    fn load_includes_only_work_on_my_plate_or_active() {
        assert!(is_load_status("my-plate"));
        assert!(is_load_status("active"));
        assert!(!is_load_status("waiting"));
        assert!(!is_load_status("submitted"));
        assert!(!is_load_status("deferred"));
    }

    #[test]
    fn main_analysis_adds_waiting_without_calling_it_load() {
        assert!(is_analysis_status("my-plate"));
        assert!(is_analysis_status("active"));
        assert!(is_analysis_status("waiting"));
        assert!(!is_analysis_status("submitted"));
        assert!(!is_load_status("waiting"));
    }

    #[test]
    fn timeline_has_one_column_per_day_and_events_stay_on_their_date() {
        let snapshots = vec![
            TimelineSnapshot {
                date: date("2026-05-01"),
                projects: vec![project("research/a.md")],
                outflow: vec![TimelineOutflow {
                    track: "research".into(),
                    status: "done".into(),
                    count: 1,
                }],
                pipeline: Vec::new(),
            },
            TimelineSnapshot {
                date: date("2026-05-03"),
                projects: vec![project("research/b.md")],
                outflow: Vec::new(),
                pipeline: Vec::new(),
            },
        ];

        let daily = fill_daily_snapshots(snapshots);

        assert_eq!(daily.len(), 3);
        assert_eq!(daily[1].date, date("2026-05-02"));
        assert_eq!(daily[1].projects[0].file, "research/a.md");
        assert!(daily[1].outflow.is_empty());
        assert_eq!(daily[2].projects[0].file, "research/b.md");
    }
}
