use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use project_hq::commands::{
    render_all, render_context, render_my_plate, render_person, render_stale, render_summary,
    render_waiting, run_init, run_new, NewOptions,
};
use project_hq::config::Config;
use project_hq::load_all;
use project_hq::project::Project;

#[derive(Parser)]
#[command(name = "hq", about = "Query HQ project-tracking files")]
struct Cli {
    /// Path to the HQ directory (default: current directory)
    #[arg(long, env = "HQ_DIR", global = true)]
    dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Projects marked as my-plate (ball in my court)
    MyPlate,
    /// Everything in waiting/submitted
    Waiting,
    /// Waiting/submitted > 30 days
    Stale,
    /// Counts by status per track
    Summary,
    /// Everything grouped by status
    All,
    /// Show available actions in a context (for example: phone or @phone)
    #[command(alias = "tag")]
    Context {
        /// Context to filter by
        name: String,
    },
    /// Show available actions involving a person or role (for example: alex or &alex)
    Person {
        /// Person or role to filter by
        name: String,
    },
    /// Start the web dashboard server
    Serve {
        /// Port to listen on
        #[arg(long, default_value = "3001")]
        port: u16,
    },
    /// Check whether the directory is a valid HQ directory (exit 0 = valid)
    Check,
    /// Create a starter HQ directory with example tracks and cards
    Init,
    /// Create a new project markdown file with frontmatter
    New {
        /// Track directory (e.g. research, funding, personal)
        track: String,
        /// Project title (also used to derive the slug unless --slug is given)
        #[arg(long)]
        title: String,
        /// Owner prefix for the filename (defaults to default_owner in hq.toml, then "yy")
        #[arg(long)]
        owner: Option<String>,
        /// Filename slug (defaults to slugified title)
        #[arg(long)]
        slug: Option<String>,
        /// Initial status
        #[arg(long, default_value = "active")]
        status: String,
        /// Priority (higher floats first)
        #[arg(long)]
        priority: Option<f64>,
        /// Deadline (free-form, e.g. 2026-06-15)
        #[arg(long)]
        deadline: Option<String>,
        /// Initial my_next field
        #[arg(long)]
        my_next: Option<String>,
        /// Checklist availability mode
        #[arg(long, value_parser = ["serial", "parallel"])]
        action_mode: Option<String>,
        /// Open $EDITOR on the new file after creation
        #[arg(long)]
        edit: bool,
        /// Create the track directory if it doesn't exist
        #[arg(long = "new-track")]
        new_track: bool,
    },
}

fn resolve_hq_dir(cli_dir: Option<PathBuf>) -> PathBuf {
    if let Some(d) = cli_dir {
        return d;
    }
    // Current directory as default; override with --dir or HQ_DIR env var
    PathBuf::from(".")
}

fn validate_hq_dir(hq_dir: &Path) -> Result<(), String> {
    if hq_dir.is_dir() {
        Ok(())
    } else {
        Err(format!(
            "HQ directory does not exist or is not a directory: {}",
            hq_dir.display()
        ))
    }
}

fn render_project_command(
    command: &Command,
    projects: &[Project],
    config: &Config,
) -> Option<String> {
    match command {
        Command::MyPlate => Some(render_my_plate(projects, config)),
        Command::Waiting => Some(render_waiting(projects)),
        Command::Stale => Some(render_stale(projects, config)),
        Command::Summary => Some(render_summary(projects, config)),
        Command::All => Some(render_all(projects, config)),
        Command::Context { name } => Some(render_context(projects, name)),
        Command::Person { name } => Some(render_person(projects, name)),
        Command::Serve { .. } | Command::Check | Command::Init | Command::New { .. } => None,
    }
}

fn main() {
    let cli = Cli::parse();
    let hq_dir = resolve_hq_dir(cli.dir);

    // `init` creates the directory itself, so it skips the existence check.
    if matches!(cli.command, Command::Init) {
        match run_init(&hq_dir) {
            Ok(created) => {
                println!("Initialized HQ in {}", hq_dir.display());
                for path in created {
                    println!("  {}", path.display());
                }
            }
            Err(message) => {
                eprintln!("{message}");
                std::process::exit(1);
            }
        }
        return;
    }

    if let Err(message) = validate_hq_dir(&hq_dir) {
        eprintln!("{message}");
        std::process::exit(2);
    }

    match &cli.command {
        Command::Serve { port } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(project_hq::web::serve(hq_dir, *port));
        }
        Command::Check => {
            let config = Config::load(&hq_dir);
            if config.tracks.is_empty() {
                eprintln!(
                    "No HQ tracks found in {}. Expected subdirectories with .md files containing YAML frontmatter.",
                    hq_dir.display()
                );
                std::process::exit(1);
            }
            println!(
                "OK: {} track(s) in {}",
                config.tracks.len(),
                hq_dir.display()
            );
        }
        Command::New {
            track,
            title,
            owner,
            slug,
            status,
            priority,
            deadline,
            my_next,
            action_mode,
            edit,
            new_track,
        } => {
            let opts = NewOptions {
                track: track.clone(),
                title: title.clone(),
                owner: owner.clone(),
                slug: slug.clone(),
                status: status.clone(),
                priority: *priority,
                deadline: deadline.clone(),
                my_next: my_next.clone(),
                action_mode: action_mode.clone(),
                edit: *edit,
                new_track: *new_track,
            };
            match run_new(&hq_dir, opts) {
                Ok(path) => println!("{}", path.display()),
                Err(message) => {
                    eprintln!("{message}");
                    std::process::exit(1);
                }
            }
        }
        command => {
            let config = Config::load(&hq_dir);
            let projects = load_all(&hq_dir, &config);

            let output = render_project_command(command, &projects, &config)
                .expect("matched reporting command");
            print!("{output}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{render_project_command, Command};
    use project_hq::action::ActionMode;
    use project_hq::config::Config;
    use project_hq::project::{Project, DEFAULT_PRIORITY};

    fn config() -> Config {
        Config {
            tracks: vec!["research".to_string()],
            skip_files: Vec::new(),
            stale_days: 30,
            statuses: vec!["my-plate".to_string(), "active".to_string()],
            default_owner: None,
            pulse_tracks: Vec::new(),
        }
    }

    fn project(status: &str) -> Project {
        Project {
            title: "Paper".to_string(),
            track: "research".to_string(),
            status: status.to_string(),
            owner: String::new(),
            priority: DEFAULT_PRIORITY,
            waiting_on: String::new(),
            waiting_since: None,
            my_next: "draft intro".to_string(),
            last: String::new(),
            deadline: None,
            deferred_until: None,
            visible: true,
            action_mode: ActionMode::Parallel,
            actions: Vec::new(),
            file: "research/paper.md".to_string(),
        }
    }

    #[test]
    fn render_project_command_dispatches_reporting_commands() {
        let config = config();
        let projects = vec![project("my-plate")];

        let output = render_project_command(&Command::MyPlate, &projects, &config).unwrap();

        assert!(output.contains("My plate (1):"));
        assert!(output.contains("Paper"));

        let output = render_project_command(
            &Command::Context {
                name: "phone".to_string(),
            },
            &projects,
            &config,
        )
        .unwrap();
        assert_eq!(output, "No available actions for @phone.\n");

        let output = render_project_command(
            &Command::Person {
                name: "alex".to_string(),
            },
            &projects,
            &config,
        )
        .unwrap();
        assert_eq!(output, "No available actions for &alex.\n");
    }

    #[test]
    fn render_project_command_excludes_operational_commands() {
        let config = config();
        let projects = vec![project("my-plate")];

        assert!(render_project_command(&Command::Check, &projects, &config).is_none());
        assert!(
            render_project_command(&Command::Serve { port: 3001 }, &projects, &config).is_none()
        );
    }
}
