use std::{collections::HashMap, path::PathBuf, str::FromStr};

use thiserror::Error;

use crate::Repository;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to create metadata directory.")]
    FailedToCreateMetadataDir(#[source] std::io::Error),
    #[error("Failed to read file '{1}'")]
    FailedToReadFile(std::io::Error, PathBuf),
    #[error("Failed to write to file '{1}'")]
    FailedToWriteFile(std::io::Error, PathBuf),
}
pub struct Metadata<'a> {
    repo: &'a Repository,
}

impl<'a> Metadata<'a> {
    pub fn new(repo: &'a Repository) -> Result<Metadata<'a>, Error> {
        let me = Metadata { repo };

        if !me.metadata_path().exists() {
            std::fs::create_dir_all(me.metadata_path())
                .map_err(Error::FailedToCreateMetadataDir)?;
        }

        Ok(me)
    }

    pub fn metadata_path(&self) -> PathBuf {
        self.repo.path().join("metadata")
    }

    fn generations_to_kv(&self) -> Result<HashMap<u64, String>, Error> {
        let generation_file_path = self.metadata_path().join("generations");
        let text = std::fs::read_to_string(&generation_file_path).unwrap_or(String::new());
        let kv = text
            .lines()
            .filter_map(|line| line.split_once(":"))
            .filter_map(|(gen, msg)| {
                let gen = u64::from_str(gen).ok()?;
                let msg = msg.to_string();
                Some((gen, msg))
            })
            .collect::<HashMap<_, _>>();
        Ok(kv)
    }
    fn generations_write_kv(&self, kv: HashMap<u64, String>) -> Result<(), Error> {
        let generation_file_path = self.metadata_path().join("generations");
        let text = kv
            .into_iter()
            .map(|(gen, msg)| format!("{gen}:{msg}\n"))
            .collect::<String>();
        std::fs::write(&generation_file_path, text)
            .map_err(|e| Error::FailedToWriteFile(e, generation_file_path))?;
        Ok(())
    }

    pub fn set_generation_description(
        &self,
        generation: u64,
        description: String,
    ) -> Result<(), Error> {
        let mut kv = self.generations_to_kv()?;
        kv.insert(generation, description);
        self.generations_write_kv(kv)?;

        Ok(())
    }
}
