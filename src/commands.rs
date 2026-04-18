use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fmt::Write;

use crate::config::Config;
use crate::project::Project;

fn ordered_statuses<'a>(
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

fn configured_track_groups<'a>(
    projects: impl IntoIterator<Item = &'a Project>,
    config: &'a Config,
) -> Vec<(&'a str, Vec<&'a Project>)> {
    let mut by_track: BTreeMap<&str, Vec<&Project>> = BTreeMap::new();

    for project in projects {
        by_track
            .entry(project.track.as_str())
            .or_default()
            .push(project);
    }

    for track_projects in by_track.values_mut() {
        sort_projects(track_projects);
    }

    let mut ordered: Vec<_> = config
        .tracks
        .iter()
        .filter_map(|track| {
            by_track
                .remove(track.as_str())
                .map(|projects| (track.as_str(), projects))
        })
        .collect();

    ordered.extend(by_track);
    ordered
}

fn sort_projects(projects: &mut Vec<&Project>) {
    projects.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.file.cmp(&b.file))
    });
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

pub fn render_my_plate(projects: &[Project], config: &Config) -> String {
    let active: Vec<_> = projects.iter().filter(|p| p.status == "active").collect();
    let mut output = format!("Active projects ({}):\n\n", active.len());

    for (track, track_projects) in configured_track_groups(active, config) {
        writeln!(&mut output, "  [{track}]").expect("writing to string cannot fail");
        for p in track_projects {
            let next = p
                .actionable_next_step()
                .map(|step| format!(" \u{2192} {step}"))
                .unwrap_or_default();
            let deadline = deadline_suffix(p);
            writeln!(&mut output, "    {}{next}{deadline}", p.title)
                .expect("writing to string cannot fail");
        }
        output.push('\n');
    }

    output
}

pub fn render_waiting(projects: &[Project]) -> String {
    let mut waiting: Vec<_> = projects.iter().filter(|p| p.is_waiting_like()).collect();
    sort_projects(&mut waiting);
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
    let mut stale: Vec<_> = projects
        .iter()
        .filter(|p| p.is_waiting_like())
        .filter_map(|p| p.waiting_days().filter(|&d| d > threshold).map(|d| (p, d)))
        .collect();

    stale.sort_by_key(|entry| Reverse(entry.1));

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

    for (track, track_projects) in configured_track_groups(projects.iter(), config) {
        let total = track_projects.len();
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for p in track_projects {
            *counts.entry(p.status.as_str()).or_insert(0) += 1;
        }
        let parts: Vec<_> = ordered_statuses(&config.statuses, counts.keys().copied())
            .into_iter()
            .filter_map(|status| counts.get(status).map(|count| format!("{status}: {count}")))
            .collect();
        writeln!(&mut output, "  {track} ({total}): {}", parts.join(", "))
            .expect("writing to string cannot fail");
    }

    output
}

pub fn render_undefer(projects: &[Project]) -> String {
    let mut ready: Vec<_> = projects
        .iter()
        .filter(|p| p.status == "deferred")
        .filter_map(|p| p.deferred_days_past().map(|d| (p, d)))
        .collect();

    ready.sort_by_key(|entry| Reverse(entry.1));

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
            if let Some(step) = p.actionable_next_step() {
                writeln!(&mut output, "    \u{2192} {step}")
                    .expect("writing to string cannot fail");
            }
            writeln!(&mut output, "    {}", p.file).expect("writing to string cannot fail");
        }
        output
    }
}

pub fn render_all(projects: &[Project], config: &Config) -> String {
    let mut by_status: BTreeMap<&str, Vec<&Project>> = BTreeMap::new();
    for p in projects {
        by_status.entry(p.status.as_str()).or_default().push(p);
    }

    for group in by_status.values_mut() {
        sort_projects(group);
    }

    let mut output = String::new();
    for status in ordered_statuses(&config.statuses, by_status.keys().copied()) {
        if let Some(group) = by_status.get(status) {
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
    fn my_plate_omits_placeholder_next_steps() {
        let mut filled = project("Paper", "research", "active");
        filled.my_next = "draft intro".to_string();

        let mut placeholder = project("Grant", "research", "active");
        placeholder.my_next = "(fill in)".to_string();

        let output = render_my_plate(&[filled, placeholder], &config(&["research"], &[], 30));
        assert!(output.contains("Paper → draft intro"));
        assert!(output.contains("Grant"));
        assert!(!output.contains("Grant →"));
    }

    #[test]
    fn my_plate_respects_configured_track_order() {
        let admin = project("Budget", "admin", "active");
        let research = project("Paper", "research", "active");

        let output = render_my_plate(&[admin, research], &config(&["research", "admin"], &[], 30));

        let research_index = output.find("[research]").unwrap();
        let admin_index = output.find("[admin]").unwrap();

        assert!(research_index < admin_index);
    }

    #[test]
    fn my_plate_appends_tracks_missing_from_config() {
        let research = project("Paper", "research", "active");
        let alias = project("Alias", "advising", "active");

        let output = render_my_plate(&[alias, research], &config(&["research"], &[], 30));

        let research_index = output.find("[research]").unwrap();
        let advising_index = output.find("[advising]").unwrap();

        assert!(research_index < advising_index);
        assert!(output.contains("Alias"));
    }

    #[test]
    fn my_plate_sorts_projects_by_priority_within_track() {
        let mut low = project("Low", "research", "active");
        low.priority = 10;
        low.my_next = "minor".to_string();

        let mut high = project("High", "research", "active");
        high.priority = 90;
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
        low.priority = 10;
        low.waiting_on = "reviewer".to_string();

        let mut high = project("High", "research", "submitted");
        high.priority = 90;
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
        low.priority = 10;

        let mut high = project("High", "research", "active");
        high.priority = 90;

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
