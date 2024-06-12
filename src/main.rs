use std::{
    io::{stdout, Write},
    path::PathBuf,
    str::FromStr,
};

use clap::{Parser, Subcommand};
use color_eyre::{eyre::Context, Result};
use dj::{
    store::{file_store::FileStore, Store},
    Repository,
};

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
    Status,
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
            let store = FileStore::new(&repo);

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
            let store = FileStore::new(&repo);
            let files = if !files.is_empty() {
                files.iter().filter_map(|f| f.canonicalize().ok()).collect()
            } else {
                store.tracked_files()?
            };
            for file in files.into_iter() {
                if !file.exists() {
                    store.mark_file_removed(&file)?;
                } else {
                    store.generate_step(&file)?;
                }
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
        Command::Status => {
            let repo = Repository::open(cli.path)?;
            let store = FileStore::new(&repo);

            enum FileStatus {
                Changed(PathBuf),
                Deleted(PathBuf),
                Created(PathBuf),
                Noop,
            }

            let tracked_files = store.tracked_files()?;
            let files: Vec<_> = tracked_files
                .into_iter()
                .map(|file| {
                    let metadata = store
                        .get_metadata(&file)
                        .expect("Tracked file has no metadata. This is contradictory.");
                    let last_hash = metadata.steps.last().map(|x| x.hash());
                    if let Some(last_hash) = last_hash {
                        if last_hash.as_bytes() == &[0; blake3::OUT_LEN] {
                            FileStatus::Noop
                        } else {
                            let Ok(new_hash) = dj::hash_file(&file) else {
                                return FileStatus::Deleted(file);
                            };
                            match new_hash == *last_hash {
                                true => FileStatus::Noop,
                                false => FileStatus::Changed(file),
                            }
                        }
                    } else {
                        FileStatus::Changed(file)
                    }
                })
                .collect();
            if !files.is_empty() {
                println!("== Changed files");

                for file in files {
                    match file {
                        FileStatus::Changed(p) => println!("M: {}", p.display()),
                        FileStatus::Deleted(p) => println!("D: {}", p.display()),
                        FileStatus::Created(p) => println!("C: {}", p.display()),
                        FileStatus::Noop => {}
                    }
                }
            }
        }
    }

    Ok(())
}

fn run_object_command(repo: Repository, command: ObjectCommand) -> Result<()> {
    match command {
        ObjectCommand::AtPath { path } => {
            let store = FileStore::new(&repo);

            let path = dj::path::RepoPath::from_str(&path)?;
            let bytes = store.read(path)?;

            stdout().write_all(&bytes)?;
        }
    }

    Ok(())
}
