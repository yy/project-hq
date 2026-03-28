use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

use project_hq::config::Config;
use project_hq::load_all;
use project_hq::project::Project;

#[derive(Parser)]
#[command(name = "hq", about = "Query HQ project-tracking files")]
struct Cli {
    /// Path to the HQ directory (default: HQ_DIR env var or ~/git/hq)
    #[arg(long, env = "HQ_DIR")]
    dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Active projects (ball in my court)
    MyPlate,
    /// Everything in waiting/submitted
    Waiting,
    /// Waiting/submitted > 30 days
    Stale,
    /// Counts by status per track
    Summary,
    /// Everything grouped by status
    All,
    /// Show deferred projects ready to resume
    Undefer,
    /// Start the web dashboard server
    Serve {
        /// Port to listen on
        #[arg(long, default_value = "3001")]
        port: u16,
    },
}

fn resolve_hq_dir(cli_dir: Option<PathBuf>) -> PathBuf {
    if let Some(d) = cli_dir {
        return d;
    }
    // Current directory as default; override with --dir or HQ_DIR env var
    PathBuf::from(".")
}

fn cmd_my_plate(projects: &[Project], config: &Config) {
    let active: Vec<_> = projects.iter().filter(|p| p.status == "active").collect();
    println!("Active projects ({}):\n", active.len());

    for track in &config.tracks {
        let track_projects: Vec<_> = active.iter().filter(|p| p.track == *track).collect();
        if track_projects.is_empty() {
            continue;
        }
        println!("  [{track}]");
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
            println!("    {}{next}{deadline}", p.title);
        }
        println!();
    }
}

fn cmd_waiting(projects: &[Project]) {
    let waiting: Vec<_> = projects
        .iter()
        .filter(|p| p.status == "waiting" || p.status == "submitted")
        .collect();
    println!("Waiting/submitted ({}):\n", waiting.len());

    for p in &waiting {
        let days = p.waiting_days().map(|d| format!(" ({d}d)")).unwrap_or_default();
        let deadline = p
            .deadline
            .as_ref()
            .map(|d| format!(" [due {d}]"))
            .unwrap_or_default();
        println!(
            "  [{}] {} \u{2014} {}{days}{deadline}",
            p.track, p.title, p.waiting_on
        );
    }
}

fn cmd_stale(projects: &[Project], config: &Config) {
    let threshold = config.stale_days;
    let mut stale: Vec<_> = projects
        .iter()
        .filter(|p| p.status == "waiting" || p.status == "submitted")
        .filter_map(|p| p.waiting_days().filter(|&d| d >= threshold).map(|d| (p, d)))
        .collect();

    stale.sort_by(|a, b| b.1.cmp(&a.1));

    if stale.is_empty() {
        println!("No projects waiting >{threshold} days (or no 'since' dates recorded yet).");
    } else {
        println!("Stale (waiting >{threshold} days): {}\n", stale.len());
        for (p, days) in &stale {
            println!(
                "  [{}] {} \u{2014} {days}d \u{2014} {}",
                p.track, p.title, p.waiting_on
            );
        }
    }
}

fn cmd_summary(projects: &[Project], config: &Config) {
    println!("Summary:\n");
    for track in &config.tracks {
        let track_projects: Vec<_> = projects.iter().filter(|p| p.track == *track).collect();
        if track_projects.is_empty() {
            continue;
        }
        let total = track_projects.len();
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for p in &track_projects {
            *counts.entry(p.status.as_str()).or_insert(0) += 1;
        }
        let parts: Vec<_> = counts.iter().map(|(s, c)| format!("{s}: {c}")).collect();
        println!("  {track} ({total}): {}", parts.join(", "));
    }
}

fn cmd_undefer(projects: &[Project]) {
    let mut ready: Vec<_> = projects
        .iter()
        .filter(|p| p.status == "deferred")
        .filter_map(|p| p.deferred_days_past().map(|d| (p, d)))
        .collect();

    ready.sort_by(|a, b| b.1.cmp(&a.1));

    if ready.is_empty() {
        println!("No deferred projects ready to resume.");
    } else {
        println!("Deferred projects ready to resume ({}):\n", ready.len());
        for (p, days) in &ready {
            let until = p
                .deferred_until
                .map(|d| d.to_string())
                .unwrap_or_default();
            let age = if *days == 0 {
                "today".to_string()
            } else {
                format!("{days}d ago")
            };
            println!(
                "  [{}] {} (deferred until {until}, {age})",
                p.track, p.title
            );
            if !p.my_next.is_empty() {
                println!("    \u{2192} {}", p.my_next);
            }
            println!("    {}", p.file);
        }
    }
}

fn cmd_all(projects: &[Project], config: &Config) {
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

    for status in order {
        if let Some(group) = by_status.get(status) {
            println!("\n{} ({}):", status.to_uppercase(), group.len());
            for p in group {
                println!("  [{}] {}", p.track, p.title);
            }
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let hq_dir = resolve_hq_dir(cli.dir);

    // `serve` launches an async web server — handle it separately
    if let Command::Serve { port } = cli.command {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(project_hq::web::serve(hq_dir, port));
        return;
    }

    let config = Config::load(&hq_dir);
    let projects = load_all(&hq_dir, &config);

    match cli.command {
        Command::MyPlate => cmd_my_plate(&projects, &config),
        Command::Waiting => cmd_waiting(&projects),
        Command::Stale => cmd_stale(&projects, &config),
        Command::Summary => cmd_summary(&projects, &config),
        Command::All => cmd_all(&projects, &config),
        Command::Undefer => cmd_undefer(&projects),
        Command::Serve { .. } => unreachable!(),
    }
}
