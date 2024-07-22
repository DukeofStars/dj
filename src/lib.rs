use std::path::PathBuf;

use clap::Args;

pub mod plumb;

#[derive(Args, Clone, Debug)]
pub struct Options {
    #[clap(default_value = ".tn")]
    pub repo_path: PathBuf,
    #[clap(long)]
    pub disable_logging: bool,
}

pub struct StorePath {
    hash: blake3::Hash,
}
impl ToString for StorePath {
    fn to_string(&self) -> String {
        self.hash.to_string()
    }
}
