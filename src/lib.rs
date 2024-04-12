use std::path::{Path, PathBuf};

use thiserror::Error;

pub struct Repository {
    path: PathBuf,
    work_dir: PathBuf,
}
impl Repository {
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
    pub fn work_dir(&self) -> &PathBuf {
        &self.work_dir
    }

    pub fn relative_path<'a>(&self, path: &'a PathBuf) -> Option<&'a Path> {
        path.strip_prefix(self.work_dir()).ok()
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Repository already exists")]
    RepositoryAlreadyExists,
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("Failed to create directory: '{}'", .0.display())]
    FailedToCreateDir(PathBuf, #[source] std::io::Error),
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

    Ok(Repository {
        path: path.canonicalize()?,
        work_dir,
    })
}
