use std::path::PathBuf;

use clap::{Parser, Subcommand};
use color_eyre::{eyre::Context, Result};
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
    Add {
        files: Vec<PathBuf>,
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
        Command::Add { files } => {
            let repo = Repository::open(cli.path)?;
            let store = Store::new(&repo);

            for file in files.iter().filter_map(|f| f.canonicalize().ok()) {
                if !store.is_tracked(&file) {
                    store
                        .begin_tracking(&file)
                        .wrap_err(format!("Failed to track '{}'", file.display()))?;
                    println!("tracking: {}", file.display());
                }
                store.add_object(&file).wrap_err(format!(
                    "Failed to add '{}' to the object store.",
                    file.display()
                ))?;
            }
        }
    }

    Ok(())
}
