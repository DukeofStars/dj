use std::{
    io::{Read},
    path::PathBuf,
};

use clap::{Parser, Subcommand};
use color_eyre::{Result};
use tn::Options;
use tracing::Level;

#[derive(Parser)]
struct Cli {
    #[clap(flatten)]
    options: tn::Options,
    #[clap(subcommand)]
    subcommand: Command,
}
#[derive(Subcommand, Clone)]
enum Command {
    Plumb {
        #[clap(subcommand)]
        command: PlumbingCommand,
    },
}
#[derive(Subcommand, Clone)]
enum PlumbingCommand {
    WriteToStore {
        #[clap(long)]
        path: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse_from(wild::args());

    if !cli.options.disable_logging {
        tracing_subscriber::fmt()
            .with_max_level(Level::TRACE)
            .without_time()
            .init();
    }

    match cli.subcommand {
        Command::Plumb { command } => run_plumbing_command(command, &cli.options)?,
    }

    Ok(())
}

fn run_plumbing_command(command: PlumbingCommand, options: &Options) -> Result<()> {
    match command {
        PlumbingCommand::WriteToStore { path } => {
            if !options.repo_path.join("store").exists() {
                std::fs::create_dir_all(&options.repo_path.join("store"))?;
            }
            if let Some(path) = path {
                tn::plumb::write_file_to_store(path, options)?;
            } else {
                let mut stdin = std::io::stdin();
                let mut bytes = Vec::new();
                stdin.read_to_end(&mut bytes)?;

                tn::plumb::write_bytes_to_store(bytes, options)?;
            }
            Ok(())
        }
    }
}
