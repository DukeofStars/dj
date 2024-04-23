use std::path::{Path, PathBuf};
use std::str::FromStr;

use base64::{engine::general_purpose::URL_SAFE, Engine};
use thiserror::Error;

pub mod metadata;
pub mod path;
pub mod store;

#[derive(Debug)]
pub struct Repository {
    path: PathBuf,
    work_dir: PathBuf,
    generation: u64,
}
impl Repository {
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
    pub fn work_dir(&self) -> &PathBuf {
        &self.work_dir
    }
    pub fn generation(&self) -> &u64 {
        &self.generation
    }

    pub fn inc_generation(&mut self) {
        self.generation += 1;
    }

    pub fn relative_path<'a>(&self, path: &'a PathBuf) -> Option<PathBuf> {
        if path.is_relative() {
            return Some(path.clone());
        }

        path.strip_prefix(self.work_dir())
            .ok()
            .map(|p| p.to_path_buf())
    }

    pub fn open(path: PathBuf) -> Result<Repository, Error> {
        if !path.exists() {
            return Err(Error::RepositoryDoesntExist);
        }
        let work_dir_file_path = path.join("working");
        let work_dir = match std::fs::read_to_string(&work_dir_file_path) {
            Ok(work_dir) => PathBuf::from(work_dir),
            _ => {
                eprintln!("No working directory found, assuming ..");
                let Some(work_dir) = path.parent() else {
                    return Err(Error::NoWorking);
                };
                work_dir.to_path_buf()
            }
        };

        let generation_file_path = path.join("generation");
        let generation = match std::fs::read_to_string(&generation_file_path) {
            Ok(generation) => {
                u64::from_str(&generation).map_err(|e| Error::ParseIntError(e, generation))?
            }
            _ => {
                eprintln!("No timestamp found, assuming generation 1");
                1
            }
        };

        Ok(Repository {
            path,
            work_dir,
            generation,
        })
    }

    fn save_state(&self) -> Result<(), Error> {
        std::fs::write(
            self.path.join("working"),
            self.work_dir().display().to_string(),
        )?;
        std::fs::write(self.path.join("generation"), format!("{}", self.generation))?;

        Ok(())
    }
}
impl Drop for Repository {
    fn drop(&mut self) {
        if self.save_state().is_err() {
            eprintln!("Failed to save repository state");
            eprintln!("debug:");
            eprintln!("{:#?}", self);
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Repository already exists")]
    RepositoryAlreadyExists,
    #[error("Repository doesn't exist")]
    RepositoryDoesntExist,
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("Failed to create directory: '{}'", .0.display())]
    FailedToCreateDir(PathBuf, #[source] std::io::Error),
    #[error("The repository has no working directory")]
    NoWorking,
    #[error("Failed to parse '{1}' as int.")]
    ParseIntError(#[source] core::num::ParseIntError, String),
}

pub fn create_repository(path: PathBuf, work_dir: PathBuf) -> Result<Repository, Error> {
    if path.exists() {
        return Err(Error::RepositoryAlreadyExists);
    }

    create_repository_force(path, work_dir)
}

pub fn create_repository_force(path: PathBuf, work_dir: PathBuf) -> Result<Repository, Error> {
    std::fs::create_dir_all(&path).map_err(|e| Error::FailedToCreateDir(path.clone(), e))?;
    if !work_dir.exists() {
        std::fs::create_dir_all(&work_dir)
            .map_err(|e| Error::FailedToCreateDir(work_dir.clone(), e))?;
    }

    let work_dir = work_dir.canonicalize()?;
    std::fs::write(path.join("working"), work_dir.display().to_string())?;

    Repository::open(path)
}

fn path_to_base64(path: &Path) -> String {
    let mut encoded = String::new();
    URL_SAFE.encode_string(path.display().to_string(), &mut encoded);
    encoded
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::Repository;

    #[test]
    pub fn relative_path_test() {
        let repo = Repository {
            path: PathBuf::from("/path/to/repository/.dj"),
            work_dir: PathBuf::from("/path/to/repository"),
            generation: 1,
        };

        let file = PathBuf::from("/path/to/repository/foo.txt");
        let sub_file = PathBuf::from("/path/to/repository/bar/foo.txt");
        let sub_dir = PathBuf::from("/path/to/repository/bar/baz/");

        assert_eq!(repo.relative_path(&file), Some(PathBuf::from("foo.txt")));
        assert_eq!(
            repo.relative_path(&sub_file),
            Some(PathBuf::from("bar/foo.txt"))
        );
        assert_eq!(
            repo.relative_path(&sub_dir),
            Some(PathBuf::from("bar/baz/"))
        );
    }
}
