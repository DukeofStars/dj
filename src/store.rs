use std::path::PathBuf;

use thiserror::Error;

use crate::Repository;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to create store directory")]
    FailedToCreateStoreDir(#[source] std::io::Error),
    #[error("Failed to create objects directory")]
    FailedToCreateObjectsDir(#[source] std::io::Error),
    #[error("Failed to write to file '{}'", .1.display())]
    FailedToWriteToFile(#[source] std::io::Error, PathBuf),
    #[error("Failed to read directory '{}'", .1.display())]
    FailedToReadDir(#[source] std::io::Error, PathBuf),
    #[error("Failed to read file '{}'", .1.display())]
    FailedToReadFile(#[source] std::io::Error, PathBuf),
    #[error("The file '{}' is not in the repository working directory.", .0.display())]
    FileNotInWorking(PathBuf),
}

pub struct Store<'repo> {
    repo: &'repo Repository,
}

impl<'repo> Store<'repo> {
    pub fn new(repo: &'repo Repository) -> Store<'repo> {
        Store { repo }
    }

    fn store_path(&self) -> PathBuf {
        self.repo.path().join("store")
    }
    fn objects_path(&self) -> PathBuf {
        self.store_path().join("objects")
    }

    fn ensure_store_path_exists(&self) -> Result<(), Error> {
        let store_path = self.store_path();
        if !store_path.exists() {
            std::fs::create_dir_all(&store_path).map_err(Error::FailedToCreateStoreDir)?;
        }
        Ok(())
    }
    fn ensure_objects_path_exists(&self) -> Result<(), Error> {
        self.ensure_store_path_exists()?;
        let objects_path = self.objects_path();
        if !objects_path.exists() {
            std::fs::create_dir_all(&objects_path).map_err(Error::FailedToCreateObjectsDir)?;
        }
        Ok(())
    }

    pub fn is_tracked(&self, path: &PathBuf) -> bool {
        let Some(path) = self.repo.relative_path(path) else {
            return false;
        };

        let path_encoded = crate::path_to_base64(&path);

        let store_path = self.store_path().join(path_encoded);

        store_path.exists()
    }

    pub fn begin_tracking(&self, path: &PathBuf) -> Result<(), Error> {
        let path = self
            .repo
            .relative_path(path)
            .ok_or(Error::FileNotInWorking(path.clone()))?;

        if path.is_dir() {
            return self.begin_tracking_dir(&path);
        }

        self.ensure_store_path_exists()?;
        let encoded_path = crate::path_to_base64(&path);
        let track_file = self.store_path().join(encoded_path);
        if track_file.exists() {
            return Ok(());
        }

        std::fs::write(&track_file, "").map_err(|e| Error::FailedToWriteToFile(e, track_file))?;

        Ok(())
    }

    fn begin_tracking_dir(&self, path: &PathBuf) -> Result<(), Error> {
        let read_dir = path
            .read_dir()
            .map_err(|e| Error::FailedToReadDir(e, path.to_path_buf()))?;
        for file in read_dir
            .into_iter()
            .filter_map(|r| r.ok())
            .filter_map(|entry| entry.path().canonicalize().ok())
        {
            self.begin_tracking(&file)?;
        }

        Ok(())
    }

    pub fn add_object(&self, path: &PathBuf) -> Result<(), Error> {
        let path = self
            .repo
            .relative_path(path)
            .ok_or(Error::FileNotInWorking(path.clone()))?;

        if path.is_dir() {
            return self.add_object_dir(&path);
        }

        let bytes = std::fs::read(&path).map_err(|e| Error::FailedToReadFile(e, path.clone()))?;
        let obj_path = crate::path::Path::new(self.repo.generation, 1, path)
            .expect("Path is already guaranteed to be relative")
            .to_store_path();
        let path = self.objects_path().join(obj_path);

        self.ensure_objects_path_exists()?;

        if !path.exists() {
            std::fs::write(&path, bytes).map_err(|e| Error::FailedToWriteToFile(e, path))?;
        }

        Ok(())
    }

    fn add_object_dir(&self, path: &PathBuf) -> Result<(), Error> {
        let read_dir = path
            .read_dir()
            .map_err(|e| Error::FailedToReadDir(e, path.to_path_buf()))?;
        for file in read_dir
            .into_iter()
            .filter_map(|r| r.ok())
            .filter_map(|entry| entry.path().canonicalize().ok())
        {
            self.add_object(&file)?;
        }

        Ok(())
    }
}
