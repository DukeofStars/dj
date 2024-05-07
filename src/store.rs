use std::path::PathBuf;

use base64::{engine::general_purpose::URL_SAFE, Engine};
use thiserror::Error;

use crate::Repository;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to create store directory")]
    FailedToCreateStoreDir(#[source] std::io::Error),
    #[error("Failed to read directory '{}'", .1.display())]
    FailedToReadDir(#[source] std::io::Error, PathBuf),
    #[error("Failed to create directory '{}'", .1.display())]
    FailedToCreateDir(#[source] std::io::Error, PathBuf),
    #[error("Failed to read file '{}'", .1.display())]
    FailedToReadFile(#[source] std::io::Error, PathBuf),
    #[error("Failed to write to file '{}'", .1.display())]
    FailedToWriteToFile(#[source] std::io::Error, PathBuf),
    #[error("The file '{}' is not in the repository working directory.", .0.display())]
    FileNotInWorking(PathBuf),
    #[error("Failed to decode base64")]
    DecodeError(#[from] base64::DecodeError),
    #[error("Invalid utf8")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}

pub struct Store<'repo> {
    repo: &'repo Repository,
}

impl<'repo> Store<'repo> {
    pub fn new(repo: &'repo Repository) -> Store<'repo> {
        Store { repo }
    }

    pub fn store_path(&self) -> PathBuf {
        self.repo.path().join("store")
    }

    fn ensure_store_path_exists(&self) -> Result<(), Error> {
        let store_path = self.store_path();
        if !store_path.exists() {
            std::fs::create_dir_all(&store_path).map_err(Error::FailedToCreateStoreDir)?;
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
    pub fn tracked_files(&self) -> Result<Vec<PathBuf>, Error> {
        self.store_path()
            .read_dir()
            .map_err(|e| Error::FailedToReadDir(e, self.store_path()))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().unwrap().is_dir())
            .map(|entry| {
                let path = entry.path();
                let filename = path.file_name().unwrap().to_str().unwrap();
                let rel_path =
                    String::from_utf8(URL_SAFE.decode(filename).map_err(Error::DecodeError)?)
                        .map_err(Error::InvalidUtf8)?;
                let rel_path = PathBuf::from(rel_path);
                Ok(rel_path)
            })
            .collect()
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
        let tracking_dir_path = self.store_path().join(encoded_path);
        if tracking_dir_path.exists() {
            return Ok(());
        }

        std::fs::create_dir_all(&tracking_dir_path)
            .map_err(|e| Error::FailedToCreateDir(e, tracking_dir_path))?;

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

    pub fn list_objects(&self) -> Result<Vec<String>, Error> {
        let Ok(read_dir) = self.store_path().read_dir() else {
            return Ok(Vec::new());
        };
        let res: Vec<String> = 
            // Iterate over entries.
            read_dir
            .into_iter()
            // Ignore invalid entries
            .filter_map(|res| res.ok())
            // Get the path
            .map(|entry| entry.path())
            // Read the directory, and get all files within
            .map(|path| {
                let read_dir = path
                    .read_dir()
                    .map_err(|e| Error::FailedToReadDir(e, path))?;

                // Return Result<Iter<Item = String>, Error>
                Ok(
                    read_dir
                        .filter_map(|res| res.ok())
                        .map(|entry| entry.path().display().to_string())
                )
            })
            // Extract any errors
            .collect::<Result<Vec<_>, Error>>()?
            // Re-iterate and flatten
            .into_iter()
            .flatten()
            // Collect into final Vec
            .collect();
        Ok(res)
    }
    fn list_objects_with_prefix(&self, prefix: &str) -> Result<Vec<String>, Error> {
        Ok(self
            .list_objects()?
            .into_iter()
            .filter(|x| x.starts_with(prefix))
            .collect())
    }

    fn get_next_object_step(&self, path: &PathBuf) -> Result<u64, Error> {
        let path = self
            .repo
            .relative_path(path)
            .ok_or(Error::FileNotInWorking(path.clone()))?;

        let prefix = 
            PathBuf::from(URL_SAFE.encode(path.display().to_string())).join(self.repo.generation().to_string());
        let objects = self.list_objects_with_prefix(&prefix)?;

        let next_step = objects.len() as u64 + 1;
        Ok(next_step)
    }

    pub fn add_object(&self, path: &PathBuf) -> Result<(), Error> {
        let src_path = self
            .repo
            .relative_path(path)
            .ok_or(Error::FileNotInWorking(path.clone()))?;

        if src_path.is_dir() {
            return self.add_object_dir(&src_path);
        }

        if !self.is_tracked(&src_path) {
            return Ok(());
        }

        self.ensure_store_path_exists()?;

        let step = self.get_next_object_step(&src_path)?;
        let obj_path = crate::path::Path::new(self.repo.generation, step, src_path.clone())
            .expect("Path is already guaranteed to be relative")
            .to_store_path();
        let out_path = self.store_path().join(obj_path);

        let bytes = if src_path.exists() {
            std::fs::read(&src_path).map_err(|e| Error::FailedToReadFile(e, out_path.clone()))?
        } else {
            // If the src_path doesn't exist (file has been deleted), just create an empty object file.
            Vec::new()
        };
        std::fs::write(&out_path, bytes).map_err(|e| Error::FailedToWriteToFile(e, out_path))?;

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
