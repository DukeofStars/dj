use std::{
    io::{stdout, Write},
    path::PathBuf,
    str::FromStr,
};

use clap::{Parser, Subcommand};
use color_eyre::{
    eyre::{eyre, Context},
    Result,
};
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
    #[clap(alias = "obj")]
    Object {
        #[clap(subcommand)]
        command: ObjectCommand,
    },
    /// Alias for 'dj obj at-path [path]'
    Cat {
        path: String,
    }
}
#[derive(Debug, Subcommand)]
enum GenerationCommand {
    Describe {
        #[clap(short, long)]
        msg: String,
    },
    New,
}
#[derive(Debug, Subcommand)]
enum ObjectCommand {
    AtPath { path: String },
    ListPaths,
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

            let path = dj::path::Path::from_str(&path)?;
            let store_path = store.objects_path().join(path.to_store_path());

            if !store_path.exists() {
                return Err(eyre!("Requested path doesn't exist."));
            }

            let bytes = std::fs::read(store_path)?;
            stdout().write_all(&bytes)?;
        }
        ObjectCommand::ListPaths => {
            let store = Store::new(&repo);

            for object in store.list_objects()? {
                if let Ok(path) = dj::path::Path::from_store_path(object) {
                    println!("{path}");
                }
            }
        }
    }

    Ok(())
}

fn run_generation_command(mut repo: Repository, command: GenerationCommand) -> Result<()> {
    match command {
        GenerationCommand::Describe { msg } => {
            let meta = Metadata::new(&repo)?;
            meta.set_generation_description(*repo.generation(), msg)?;
        }
        GenerationCommand::New => {
            repo.inc_generation();
        }
    }

    Ok(())
}
