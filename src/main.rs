use std::{
    io::{stdout, Write},
    path::PathBuf,
    str::FromStr,
};

use clap::{Parser, Subcommand};
use color_eyre::{eyre::WrapErr, Result};
use dj::{store::Store, Repository};

#[derive(Debug, Parser)]
struct Cli {
    #[clap(subcommand)]
    command: Command,
    #[clap(short, long, default_value = ".dj/")]
    path: PathBuf,
}
#[derive(Debug, Subcommand)]
enum Command {
    Init {
        #[clap(default_value = ".dj/")]
        path: PathBuf,
        #[clap(long, default_value = ".")]
        work_dir: PathBuf,
        #[clap(short, long)]
        force: bool,
    },
    Track {
        files: Vec<PathBuf>,
    },
    Step {
        files: Vec<PathBuf>,
    },
    #[clap(alias = "obj")]
    Object {
        #[clap(subcommand)]
        command: ObjectCommand,
    },
    /// Alias for 'dj obj at-path [path]'
    Cat {
        path: String,
    },
}
#[derive(Debug, Subcommand)]
enum ObjectCommand {
    AtPath { path: String },
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse_from(wild::args());

    match cli.command {
        Command::Init {
            path,
            work_dir,
            force,
        } => {
            let repository = match force {
                true => dj::create_repository_force(path, work_dir)?,
                false => dj::create_repository(path, work_dir)?,
            };
            eprintln!(
                "Created new repository in: '{}'",
                repository.path().display()
            );
            eprintln!("working in: '{}'", repository.work_dir().display());
        }
        Command::Track { files } => {
            let repo = Repository::open(cli.path)?;
            let store = Store::new(&repo);

            for file in files.iter().filter_map(|f| f.canonicalize().ok()) {
                if !store.is_tracked(&file) {
                    store
                        .begin_tracking(&file)
                        .wrap_err(format!("Failed to track '{}'", file.display()))?;
                }
            }
        }
        Command::Step { files } => {
            let repo = Repository::open(cli.path)?;
            let store = Store::new(&repo);
            let files = if !files.is_empty() {
                files.iter().filter_map(|f| f.canonicalize().ok()).collect()
            } else {
                store.tracked_files()?
            };
            for file in files.into_iter() {
                store.generate_step(&file)?;
            }
        }
        Command::Object { command } => {
            let repo = Repository::open(cli.path)?;
            run_object_command(repo, command)?;
        }
        Command::Cat { path } => {
            let repo = Repository::open(cli.path)?;
            let command = ObjectCommand::AtPath { path };
            run_object_command(repo, command)?;
        }
    }

    Ok(())
}

fn run_object_command(repo: Repository, command: ObjectCommand) -> Result<()> {
    match command {
        ObjectCommand::AtPath { path } => {
            let store = Store::new(&repo);

            let path = dj::path::RepoPath::from_str(&path)?;
            let bytes = store.read(path)?;

            stdout().write_all(&bytes)?;
        }
    }

    Ok(())
}
