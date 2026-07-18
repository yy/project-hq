use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fmt::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;
use crate::project::Project;
use crate::project_file::{
    create_new_project, create_track, resolve_filename, slugify, validate_name, ProjectFileError,
};

const DEFAULT_OWNER_FALLBACK: &str = "yy";

pub struct NewOptions {
    pub track: String,
    pub title: String,
    pub owner: Option<String>,
    pub slug: Option<String>,
    pub status: String,
    pub priority: Option<f64>,
    pub deadline: Option<String>,
    pub my_next: Option<String>,
    pub edit: bool,
    pub new_track: bool,
}

#[derive(Debug)]
pub enum NewProjectError {
    Validation(String),
    UnknownTrack { track: String, known: String },
    ProjectFile(ProjectFileError),
}

impl fmt::Display for NewProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) => write!(f, "{message}"),
            Self::UnknownTrack { track, known } => write!(
                f,
                "Unknown track {track:?}. Existing tracks: {known}. Pass --new-track to create it."
            ),
            Self::ProjectFile(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for NewProjectError {}

impl From<ProjectFileError> for NewProjectError {
    fn from(error: ProjectFileError) -> Self {
        Self::ProjectFile(error)
    }
}

/// Create a new project from CLI options. Returns the path to the new file.
pub fn run_new(hq_dir: &Path, opts: NewOptions) -> Result<PathBuf, NewProjectError> {
    let mut config = Config::load(hq_dir);

    let title = opts.title.trim();
    if title.is_empty() {
        return Err(NewProjectError::Validation(
            "--title cannot be empty".to_string(),
        ));
    }

    let owner = opts
        .owner
        .clone()
        .or_else(|| config.default_owner.clone())
        .unwrap_or_else(|| DEFAULT_OWNER_FALLBACK.to_string());
    validate_name("owner", &owner)?;

    let slug = match opts.slug.clone() {
        Some(s) => s,
        None => {
            let derived = slugify(title);
            if derived.is_empty() {
                return Err(NewProjectError::Validation(format!(
                    "Could not derive a slug from title {title:?}; pass --slug explicitly"
                )));
            }
            derived
        }
    };
    validate_name("slug", &slug)?;

    if opts.status.trim().is_empty() {
        return Err(NewProjectError::Validation(
            "--status cannot be empty".to_string(),
        ));
    }
    if opts.priority.is_some_and(|priority| !priority.is_finite()) {
        return Err(NewProjectError::Validation(
            "--priority must be a finite number".to_string(),
        ));
    }

    let track = opts.track.clone();
    let track_dir_exists = hq_dir.join(&track).is_dir();
    if !track_dir_exists {
        if opts.new_track {
            create_track(hq_dir, &track)?;
        } else {
            let known = if config.tracks.is_empty() {
                "(no tracks discovered)".to_string()
            } else {
                config.tracks.join(", ")
            };
            return Err(NewProjectError::UnknownTrack { track, known });
        }
    }
    // Make sure the track is in the collision-scan list even if not auto-discovered yet.
    if !config.tracks.iter().any(|t| t == &track) {
        config.tracks.push(track.clone());
    }

    let filename = resolve_filename(hq_dir, &config.tracks, &owner, &slug);

    let mut fields: Vec<(String, String)> = vec![
        ("title".to_string(), title.to_string()),
        ("status".to_string(), opts.status.clone()),
    ];
    if let Some(p) = opts.priority {
        fields.push(("priority".to_string(), format_priority(p)));
    }
    if let Some(d) = opts.deadline.as_ref().filter(|s| !s.trim().is_empty()) {
        fields.push(("deadline".to_string(), d.clone()));
    }
    if let Some(n) = opts.my_next.as_ref().filter(|s| !s.trim().is_empty()) {
        fields.push(("my_next".to_string(), n.clone()));
    }

    let path = create_new_project(hq_dir, &track, &filename, &fields, "")?;

    if opts.edit {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
        let status = Command::new(&editor).arg(&path).status();
        if let Err(err) = status {
            eprintln!("warning: failed to launch {editor}: {err}");
        }
    }

    Ok(path)
}

struct StarterProject {
    track: &'static str,
    filename: &'static str,
    frontmatter: Vec<(String, String)>,
    body: &'static str,
}

/// Starter content written by `hq init`.
fn starter_projects() -> Vec<StarterProject> {
    let fields = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };

    vec![
        StarterProject {
            track: "projects",
            filename: "welcome-to-hq.md",
            frontmatter: fields(&[
                ("title", "Welcome to HQ 👋"),
                ("status", "active"),
                ("priority", "10"),
                ("my_next", "read this card, then drag it to Done"),
            ]),
            body: "Every card on this board is a plain Markdown file in your HQ folder — \
this one is `projects/welcome-to-hq.md`. There is no database: your stuff is just \
text files you can open, edit, and back up like anything else.\n\
\n\
## The basics\n\
\n\
- Columns are statuses. Drag a card to move it.\n\
- Click a card to read or edit it in the side panel.\n\
- **+ New project** adds a card to a track (a folder, like `classes/`).\n\
- Checkboxes in a card are clickable from the board:\n\
  - [ ] try checking this box\n\
  - [ ] then drag this card to **done**\n\
\n\
## The fields at the top\n\
\n\
The block between the `---` lines is what the board reads: `title`, `status`, \
and optional extras like `deadline`, `my_next` (your next action), or \
`waiting_on` (who or what you're waiting for).\n\
\n\
## Editing with Obsidian (optional, but nice)\n\
\n\
Because everything is plain Markdown, you can open your HQ folder as a vault in \
[Obsidian](https://obsidian.md) and do your longer writing there. The board \
watches the files and updates live — HQ is the dashboard, Obsidian is the editor.\n",
        },
        StarterProject {
            track: "classes",
            filename: "example-class.md",
            frontmatter: fields(&[
                ("title", "Example: Bio 101"),
                ("status", "active"),
                ("my_next", "read chapter 3 before Friday"),
                ("deadline", "friday"),
            ]),
            body: "A track like `classes/` works well with one card per course.\n\
\n\
- [ ] problem set 2\n\
- [ ] reading response\n\
- [x] lab safety quiz\n",
        },
        StarterProject {
            track: "life",
            filename: "example-waiting.md",
            frontmatter: fields(&[
                ("title", "Example: passport renewal"),
                ("status", "waiting"),
                ("waiting_on", "new photo appointment"),
            ]),
            body: "Cards in **waiting** are things where the ball is in someone else's \
court. `waiting_on` says what you're waiting for, so the board can remind you \
when it's been sitting too long.\n",
        },
    ]
}

const STARTER_README: &str = "# HQ\n\
\n\
This folder is your HQ. Each subfolder is a *track* (a category), and each\n\
Markdown file inside a track is one project card on the board.\n\
\n\
- Edit any card in any text editor — or open this folder as an Obsidian vault.\n\
- The HQ app watches these files and updates the board live.\n\
- The `---` block at the top of each card holds the fields the board reads:\n\
  `title`, `status` (active, waiting, submitted, deferred, done, dropped),\n\
  and optional `deadline`, `my_next`, `waiting_on`, `priority`.\n";

/// Create and seed a starter HQ directory. Creates `hq_dir` if needed and
/// refuses to touch a directory that already contains HQ tracks.
pub fn run_init(hq_dir: &Path) -> Result<Vec<PathBuf>, NewProjectError> {
    if hq_dir.exists() && !hq_dir.is_dir() {
        return Err(NewProjectError::Validation(format!(
            "{} exists and is not a directory",
            hq_dir.display()
        )));
    }
    std::fs::create_dir_all(hq_dir).map_err(|source| {
        NewProjectError::ProjectFile(ProjectFileError::Write {
            file: hq_dir.display().to_string(),
            source,
        })
    })?;

    let config = Config::load(hq_dir);
    if !config.tracks.is_empty() {
        return Err(NewProjectError::Validation(format!(
            "{} already looks like an HQ directory (tracks: {}); refusing to add starter content",
            hq_dir.display(),
            config.tracks.join(", ")
        )));
    }

    let mut created = Vec::new();
    for starter in starter_projects() {
        match create_track(hq_dir, starter.track) {
            Ok(()) | Err(ProjectFileError::AlreadyExists { .. }) => {}
            Err(error) => return Err(error.into()),
        }
        created.push(create_new_project(
            hq_dir,
            starter.track,
            starter.filename,
            &starter.frontmatter,
            starter.body,
        )?);
    }

    let readme = hq_dir.join("README.md");
    if !readme.exists() {
        std::fs::write(&readme, STARTER_README).map_err(|source| {
            NewProjectError::ProjectFile(ProjectFileError::Write {
                file: "README.md".to_string(),
                source,
            })
        })?;
        created.push(readme);
    }

    Ok(created)
}

fn format_priority(p: f64) -> String {
    if p.fract() == 0.0 && p.is_finite() {
        format!("{}", p as i64)
    } else {
        format!("{p}")
    }
}

fn ordered_keys<'a>(
    configured: &'a [String],
    present: impl IntoIterator<Item = &'a str>,
) -> Vec<&'a str> {
    let mut ordered: Vec<&str> = configured.iter().map(|status| status.as_str()).collect();

    for status in present {
        if !ordered.contains(&status) {
            ordered.push(status);
        }
    }

    ordered
}

fn sort_projects(projects: &mut Vec<&Project>) {
    projects.sort_by(|a, b| {
        b.priority
            .total_cmp(&a.priority)
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.file.cmp(&b.file))
    });
}

fn collect_sorted_projects<'a>(
    projects: impl IntoIterator<Item = &'a Project>,
) -> Vec<&'a Project> {
    let mut collected: Vec<_> = projects.into_iter().collect();
    sort_projects(&mut collected);
    collected
}

fn collect_projects_by_days<'a>(
    projects: impl IntoIterator<Item = &'a Project>,
    days: impl Fn(&Project) -> Option<i64>,
) -> Vec<(&'a Project, i64)> {
    let mut collected: Vec<_> = projects
        .into_iter()
        .filter_map(|project| days(project).map(|days| (project, days)))
        .collect();
    collected.sort_by_key(|entry| Reverse(entry.1));
    collected
}

fn track_key(project: &Project) -> &str {
    project.track.as_str()
}

fn status_key(project: &Project) -> &str {
    project.status.as_str()
}

fn ordered_project_groups_by<'a>(
    projects: impl IntoIterator<Item = &'a Project>,
    configured: &'a [String],
    key_for: fn(&'a Project) -> &'a str,
) -> Vec<(&'a str, Vec<&'a Project>)> {
    let mut groups: BTreeMap<&str, Vec<&Project>> = BTreeMap::new();

    for project in projects {
        groups.entry(key_for(project)).or_default().push(project);
    }

    for group in groups.values_mut() {
        sort_projects(group);
    }

    ordered_keys(configured, groups.keys().copied())
        .into_iter()
        .filter_map(|key| groups.remove(key).map(|projects| (key, projects)))
        .collect()
}

fn waiting_like_projects(projects: &[Project]) -> Vec<&Project> {
    collect_sorted_projects(projects.iter().filter(|project| project.is_waiting_like()))
}

fn stale_waiting_projects(projects: &[Project], threshold: i64) -> Vec<(&Project, i64)> {
    collect_projects_by_days(
        projects.iter().filter(|project| project.is_waiting_like()),
        |project| project.waiting_days().filter(|&days| days > threshold),
    )
}

fn ready_deferred_projects(projects: &[Project]) -> Vec<(&Project, i64)> {
    collect_projects_by_days(
        projects
            .iter()
            .filter(|project| project.status == "deferred"),
        Project::deferred_days_past,
    )
}

fn deadline_suffix(project: &Project) -> String {
    project
        .deadline
        .as_ref()
        .map(|deadline| format!(" [due {deadline}]"))
        .unwrap_or_default()
}

fn waiting_days_suffix(project: &Project) -> String {
    project
        .waiting_days()
        .map(|days| format!(" ({days}d)"))
        .unwrap_or_default()
}

fn next_step_suffix(project: &Project) -> String {
    project
        .actionable_next_step()
        .map(|step| format!(" \u{2192} {step}"))
        .unwrap_or_default()
}

fn write_next_step_line(output: &mut String, project: &Project) {
    if let Some(step) = project.actionable_next_step() {
        writeln!(output, "    \u{2192} {step}").expect("writing to string cannot fail");
    }
}

pub fn render_my_plate(projects: &[Project], config: &Config) -> String {
    let my_plate: Vec<_> = projects.iter().filter(|p| p.status == "my-plate").collect();
    let mut output = format!("My plate ({}):\n\n", my_plate.len());

    for (track, track_projects) in ordered_project_groups_by(my_plate, &config.tracks, track_key) {
        writeln!(&mut output, "  [{track}]").expect("writing to string cannot fail");
        for p in track_projects {
            let next = next_step_suffix(p);
            let deadline = deadline_suffix(p);
            writeln!(&mut output, "    {}{next}{deadline}", p.title)
                .expect("writing to string cannot fail");
        }
        output.push('\n');
    }

    output
}

pub fn render_waiting(projects: &[Project]) -> String {
    let waiting = waiting_like_projects(projects);
    let mut output = format!("Waiting/submitted ({}):\n\n", waiting.len());

    for p in waiting {
        let days = waiting_days_suffix(p);
        let deadline = deadline_suffix(p);
        writeln!(
            &mut output,
            "  [{}] {} \u{2014} {}{days}{deadline}",
            p.track, p.title, p.waiting_on
        )
        .expect("writing to string cannot fail");
    }

    output
}

pub fn render_stale(projects: &[Project], config: &Config) -> String {
    let threshold = config.stale_days;
    let stale = stale_waiting_projects(projects, threshold);

    if stale.is_empty() {
        format!("No projects waiting >{threshold} days (or no 'since' dates recorded yet).\n")
    } else {
        let mut output = format!("Stale (waiting >{threshold} days): {}\n\n", stale.len());
        for (p, days) in stale {
            writeln!(
                &mut output,
                "  [{}] {} \u{2014} {days}d \u{2014} {}",
                p.track, p.title, p.waiting_on
            )
            .expect("writing to string cannot fail");
        }
        output
    }
}

pub fn render_summary(projects: &[Project], config: &Config) -> String {
    let mut output = String::from("Summary:\n\n");

    for (track, track_projects) in
        ordered_project_groups_by(projects.iter(), &config.tracks, track_key)
    {
        let total = track_projects.len();
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for p in track_projects {
            *counts.entry(p.status.as_str()).or_insert(0) += 1;
        }
        let parts: Vec<_> = ordered_keys(&config.statuses, counts.keys().copied())
            .into_iter()
            .filter_map(|status| counts.get(status).map(|count| format!("{status}: {count}")))
            .collect();
        writeln!(&mut output, "  {track} ({total}): {}", parts.join(", "))
            .expect("writing to string cannot fail");
    }

    output
}

pub fn render_undefer(projects: &[Project]) -> String {
    let ready = ready_deferred_projects(projects);

    if ready.is_empty() {
        "No deferred projects ready to resume.\n".to_string()
    } else {
        let mut output = format!("Deferred projects ready to resume ({}):\n\n", ready.len());
        for (p, days) in ready {
            let until = p.deferred_until.map(|d| d.to_string()).unwrap_or_default();
            let age = if days == 0 {
                "today".to_string()
            } else {
                format!("{days}d ago")
            };
            writeln!(
                &mut output,
                "  [{}] {} (deferred until {until}, {age})",
                p.track, p.title
            )
            .expect("writing to string cannot fail");
            write_next_step_line(&mut output, p);
            writeln!(&mut output, "    {}", p.file).expect("writing to string cannot fail");
        }
        output
    }
}

pub fn render_all(projects: &[Project], config: &Config) -> String {
    let mut output = String::new();
    for (status, group) in ordered_project_groups_by(projects.iter(), &config.statuses, status_key)
    {
        writeln!(
            &mut output,
            "\n{} ({}):",
            status.to_uppercase(),
            group.len()
        )
        .expect("writing to string cannot fail");
        for p in group {
            writeln!(&mut output, "  [{}] {}", p.track, p.title)
                .expect("writing to string cannot fail");
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Local, NaiveDate};

    use super::{
        render_all, render_my_plate, render_stale, render_summary, render_undefer, render_waiting,
    };
    use crate::config::Config;
    use crate::project::{Project, DEFAULT_PRIORITY};

    fn config(tracks: &[&str], statuses: &[&str], stale_days: i64) -> Config {
        Config {
            tracks: tracks.iter().map(|track| track.to_string()).collect(),
            skip_files: Vec::new(),
            stale_days,
            statuses: statuses.iter().map(|status| status.to_string()).collect(),
            default_owner: None,
            pulse_tracks: Vec::new(),
        }
    }

    fn project(title: &str, track: &str, status: &str) -> Project {
        Project {
            title: title.to_string(),
            track: track.to_string(),
            status: status.to_string(),
            owner: String::new(),
            priority: DEFAULT_PRIORITY,
            waiting_on: String::new(),
            waiting_since: None,
            my_next: String::new(),
            last: String::new(),
            deadline: None,
            deferred_until: None,
            file: format!("{track}/{title}.md"),
        }
    }

    #[test]
    fn my_plate_shows_only_my_plate_status_projects() {
        let mut on_plate = project("Urgent", "research", "my-plate");
        on_plate.my_next = "send comments".to_string();
        let active = project("Active", "research", "active");

        let output = render_my_plate(&[on_plate, active], &config(&["research"], &[], 30));

        assert!(output.contains("My plate (1):"));
        assert!(output.contains("Urgent → send comments"));
        assert!(!output.contains("Active"));
    }

    #[test]
    fn my_plate_omits_placeholder_next_steps() {
        let mut filled = project("Paper", "research", "my-plate");
        filled.my_next = "draft intro".to_string();

        let mut placeholder = project("Grant", "research", "my-plate");
        placeholder.my_next = "(fill in)".to_string();

        let output = render_my_plate(&[filled, placeholder], &config(&["research"], &[], 30));
        assert!(output.contains("Paper → draft intro"));
        assert!(output.contains("Grant"));
        assert!(!output.contains("Grant →"));
    }

    #[test]
    fn my_plate_trims_next_steps_before_rendering() {
        let mut project = project("Paper", "research", "my-plate");
        project.my_next = "  draft intro  ".to_string();

        let output = render_my_plate(&[project], &config(&["research"], &[], 30));

        assert!(output.contains("Paper → draft intro"));
        assert!(!output.contains("  draft intro  "));
    }

    #[test]
    fn my_plate_respects_configured_track_order() {
        let admin = project("Budget", "admin", "my-plate");
        let research = project("Paper", "research", "my-plate");

        let output = render_my_plate(&[admin, research], &config(&["research", "admin"], &[], 30));

        let research_index = output.find("[research]").unwrap();
        let admin_index = output.find("[admin]").unwrap();

        assert!(research_index < admin_index);
    }

    #[test]
    fn my_plate_appends_tracks_missing_from_config() {
        let research = project("Paper", "research", "my-plate");
        let alias = project("Alias", "advising", "my-plate");

        let output = render_my_plate(&[alias, research], &config(&["research"], &[], 30));

        let research_index = output.find("[research]").unwrap();
        let advising_index = output.find("[advising]").unwrap();

        assert!(research_index < advising_index);
        assert!(output.contains("Alias"));
    }

    #[test]
    fn my_plate_sorts_projects_by_priority_within_track() {
        let mut low = project("Low", "research", "my-plate");
        low.priority = 10.0;
        low.my_next = "minor".to_string();

        let mut high = project("High", "research", "my-plate");
        high.priority = 90.0;
        high.my_next = "major".to_string();

        let output = render_my_plate(&[low, high], &config(&["research"], &[], 30));
        let high_index = output.find("High").unwrap();
        let low_index = output.find("Low").unwrap();

        assert!(high_index < low_index);
    }

    #[test]
    fn stale_sorts_longest_waiting_first() {
        let mut newer = project("Recent", "research", "waiting");
        newer.waiting_on = "reviewer".to_string();
        newer.waiting_since = Some(NaiveDate::from_ymd_opt(2026, 4, 1).unwrap());

        let mut older = project("Old", "research", "submitted");
        older.waiting_on = "committee".to_string();
        older.waiting_since = Some(NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());

        let output = render_stale(&[newer, older], &config(&["research"], &[], 1));
        let old_index = output.find("Old").unwrap();
        let recent_index = output.find("Recent").unwrap();

        assert!(old_index < recent_index);
    }

    #[test]
    fn waiting_sorts_projects_by_priority() {
        let mut low = project("Low", "research", "waiting");
        low.priority = 10.0;
        low.waiting_on = "reviewer".to_string();

        let mut high = project("High", "research", "submitted");
        high.priority = 90.0;
        high.waiting_on = "committee".to_string();

        let output = render_waiting(&[low, high]);
        let high_index = output.find("High").unwrap();
        let low_index = output.find("Low").unwrap();

        assert!(high_index < low_index);
    }

    #[test]
    fn stale_excludes_projects_waiting_exactly_at_threshold() {
        let threshold = 30;
        let mut exact = project("Exact", "research", "waiting");
        exact.waiting_on = "reviewer".to_string();
        exact.waiting_since = Some(Local::now().date_naive() - Duration::days(threshold));

        let output = render_stale(&[exact], &config(&["research"], &[], threshold));

        assert!(!output.contains("Exact"));
        assert_eq!(
            output,
            "No projects waiting >30 days (or no 'since' dates recorded yet).\n"
        );
    }

    #[test]
    fn undefer_omits_placeholder_next_steps() {
        let mut placeholder = project("Grant", "research", "deferred");
        placeholder.deferred_until = Some(Local::now().date_naive() - Duration::days(1));
        placeholder.my_next = "(fill in)".to_string();

        let output = render_undefer(&[placeholder]);
        assert!(output.contains("Grant"));
        assert!(!output.contains("→ (fill in)"));
    }

    #[test]
    fn undefer_shows_real_next_steps() {
        let mut project = project("Paper", "research", "deferred");
        project.deferred_until = Some(Local::now().date_naive());
        project.my_next = "restart revisions".to_string();

        let output = render_undefer(&[project]);
        assert!(output.contains("→ restart revisions"));
        assert!(output.contains("today"));
    }

    #[test]
    fn all_respects_status_order_then_appends_unknown_statuses() {
        let active = project("Alpha", "research", "active");
        let done = project("Beta", "research", "done");
        let blocked = project("Gamma", "research", "blocked");

        let output = render_all(
            &[blocked, done, active],
            &config(&["research"], &["active", "done"], 30),
        );

        let active_index = output.find("ACTIVE").unwrap();
        let done_index = output.find("DONE").unwrap();
        let blocked_index = output.find("BLOCKED").unwrap();

        assert!(active_index < done_index);
        assert!(done_index < blocked_index);
    }

    #[test]
    fn all_sorts_projects_by_priority_within_status() {
        let mut low = project("Low", "research", "active");
        low.priority = 10.0;

        let mut high = project("High", "research", "active");
        high.priority = 90.0;

        let output = render_all(&[low, high], &config(&["research"], &["active"], 30));
        let high_index = output.find("High").unwrap();
        let low_index = output.find("Low").unwrap();

        assert!(high_index < low_index);
    }

    #[test]
    fn summary_respects_status_order_then_appends_unknown_statuses() {
        let active = project("Alpha", "research", "active");
        let done = project("Beta", "research", "done");
        let blocked = project("Gamma", "research", "blocked");

        let output = render_summary(
            &[blocked, done, active],
            &config(&["research"], &["done", "active"], 30),
        );

        let done_index = output.find("done: 1").unwrap();
        let active_index = output.find("active: 1").unwrap();
        let blocked_index = output.find("blocked: 1").unwrap();

        assert!(done_index < active_index);
        assert!(active_index < blocked_index);
    }

    #[test]
    fn summary_appends_tracks_missing_from_config() {
        let research = project("Alpha", "research", "active");
        let advising = project("Beta", "advising", "waiting");

        let output = render_summary(&[advising, research], &config(&["research"], &[], 30));

        let research_index = output.find("research (1):").unwrap();
        let advising_index = output.find("advising (1):").unwrap();

        assert!(research_index < advising_index);
        assert!(output.contains("waiting: 1"));
    }
}
