use std::path::PathBuf;

use clap::{Parser, Subcommand};
use color_eyre::{eyre::Context, Result};
use dj::{metadata::Metadata, store::Store, Repository};

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
    #[clap(alias = "gen")]
    Generation {
        #[clap(subcommand)]
        command: GenerationCommand,
    },
}
#[derive(Debug, Subcommand)]
enum GenerationCommand {
    Describe {
        #[clap(short, long)]
        msg: String,
    },
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
            println!(
                "Created new repository in: '{}'",
                repository.path().display()
            );
            println!("working in: '{}'", repository.work_dir().display());
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

            for file in files.iter().filter_map(|f| f.canonicalize().ok()) {
                // Currently we assume the files always have changes.
                // In the future, we should check to see if they have actually changed.
                store.add_object(&file)?;
            }
        }
        Command::Generation { command } => {
            let repo = Repository::open(cli.path)?;
            run_generation_command(repo, command)?;
        }
    }

    Ok(())
}

fn run_generation_command(repo: Repository, command: GenerationCommand) -> Result<()> {
    match command {
        GenerationCommand::Describe { msg } => {
            let meta = Metadata::new(&repo)?;
            meta.set_generation_description(*repo.generation(), msg)?;
        }
    }

    Ok(())
}
