use std::path::PathBuf;

use clap::{Parser, Subcommand};

use project_hq::commands::{
    render_all, render_my_plate, render_stale, render_summary, render_undefer, render_waiting,
};
use project_hq::config::Config;
use project_hq::load_all;

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

fn main() {
    let cli = Cli::parse();
    let hq_dir = resolve_hq_dir(cli.dir);

    match cli.command {
        Command::Serve { port } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(project_hq::web::serve(hq_dir, port));
        }
        command => {
            let config = Config::load(&hq_dir);
            let projects = load_all(&hq_dir, &config);

            let output = match command {
                Command::MyPlate => render_my_plate(&projects, &config),
                Command::Waiting => render_waiting(&projects),
                Command::Stale => render_stale(&projects, &config),
                Command::Summary => render_summary(&projects, &config),
                Command::All => render_all(&projects, &config),
                Command::Undefer => render_undefer(&projects),
                Command::Serve { .. } => unreachable!(),
            };
            print!("{output}");
        }
    }
}
