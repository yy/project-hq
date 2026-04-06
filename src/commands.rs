use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fmt::Write;

use crate::config::Config;
use crate::project::Project;

pub fn render_my_plate(projects: &[Project], config: &Config) -> String {
    let active: Vec<_> = projects.iter().filter(|p| p.status == "active").collect();
    let mut output = format!("Active projects ({}):\n\n", active.len());

    for track in &config.tracks {
        let track_projects: Vec<_> = active.iter().filter(|p| p.track == *track).collect();
        if track_projects.is_empty() {
            continue;
        }
        writeln!(&mut output, "  [{track}]").expect("writing to string cannot fail");
        for p in track_projects {
            let next = if !p.my_next.is_empty() && p.my_next != "(fill in)" {
                format!(" \u{2192} {}", p.my_next)
            } else {
                String::new()
            };
            let deadline = p
                .deadline
                .as_ref()
                .map(|d| format!(" [due {d}]"))
                .unwrap_or_default();
            writeln!(&mut output, "    {}{next}{deadline}", p.title)
                .expect("writing to string cannot fail");
        }
        output.push('\n');
    }

    output
}

pub fn render_waiting(projects: &[Project]) -> String {
    let waiting: Vec<_> = projects.iter().filter(|p| p.is_waiting_like()).collect();
    let mut output = format!("Waiting/submitted ({}):\n\n", waiting.len());

    for p in waiting {
        let days = p
            .waiting_days()
            .map(|d| format!(" ({d}d)"))
            .unwrap_or_default();
        let deadline = p
            .deadline
            .as_ref()
            .map(|d| format!(" [due {d}]"))
            .unwrap_or_default();
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
        .filter_map(|p| p.waiting_days().filter(|&d| d >= threshold).map(|d| (p, d)))
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

    for track in &config.tracks {
        let track_projects: Vec<_> = projects.iter().filter(|p| p.track == *track).collect();
        if track_projects.is_empty() {
            continue;
        }
        let total = track_projects.len();
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for p in track_projects {
            *counts.entry(p.status.as_str()).or_insert(0) += 1;
        }
        let mut ordered_statuses: Vec<&str> = config
            .statuses
            .iter()
            .map(|status| status.as_str())
            .filter(|status| counts.contains_key(status))
            .collect();
        for status in counts.keys() {
            if !ordered_statuses.contains(status) {
                ordered_statuses.push(status);
            }
        }
        let parts: Vec<_> = ordered_statuses
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
            if !p.my_next.is_empty() {
                writeln!(&mut output, "    \u{2192} {}", p.my_next)
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

    let mut order: Vec<&str> = config
        .statuses
        .iter()
        .map(|s| s.as_str())
        .filter(|s| by_status.contains_key(s))
        .collect();
    for key in by_status.keys() {
        if !order.contains(key) {
            order.push(key);
        }
    }

    let mut output = String::new();
    for status in order {
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
    use chrono::NaiveDate;

    use super::{render_all, render_my_plate, render_stale, render_summary};
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
}
