use std::path::PathBuf;

use blake3::Hash;

use crate::path::{ObjectPath, RepoPath};

pub mod file_store;

pub trait Store {
    type Error: std::error::Error;

    /// Returns a list of relative paths to the working dir root.
    /// The list is of all the paths that are tracked.
    fn tracked_files(&self) -> Result<Vec<PathBuf>, Self::Error>;

    /// Returns whether or not a file is tracked.
    fn is_tracked(&self, path: &PathBuf) -> bool {
        let Ok(tracked_files) = self.tracked_files() else {
            return false;
        };
        tracked_files.into_iter().any(|p| p == *path)
    }

    /// Begin tracking a file in the working directory.
    /// Only creates the metadata.
    fn begin_tracking(&self, path: &PathBuf) -> Result<(), Self::Error>;

    /// Get the metadata for a path in the working directory.
    fn get_metadata(&self, path: &PathBuf) -> Result<FileMeta, Self::Error>;

    /// Write metadata for a path in the working directory.
    fn write_metadata(&self, path: &PathBuf, meta: FileMeta) -> Result<(), Self::Error>;

    /// Generate a new step for a file, reading new file contents and updating the file metadata.
    fn generate_step(&self, path: &PathBuf) -> Result<ObjectPath, Self::Error>;

    /// Read a file from the object store.
    fn read(&self, path: impl AsRef<RepoPath>) -> Result<Vec<u8>, Self::Error>;

    /// Write a file to the object store.
    fn write<C: AsRef<[u8]>>(
        &self,
        path: impl AsRef<RepoPath>,
        content: C,
    ) -> Result<(), Self::Error>;

    /// Add a step to a files metadata.
    fn add_step_to_metadata(&self, path: &PathBuf, step: ObjectPath) -> Result<(), Self::Error> {
        let mut meta = self.get_metadata(path)?;
        meta.steps.push(step);

        self.write_metadata(path, meta)?;
        Ok(())
    }

    ///
    fn mark_file_removed(&self, path: &PathBuf) -> Result<(), Self::Error> {
        let mut metadata = self.get_metadata(path)?;
        metadata
            .steps
            .push(ObjectPath(Hash::from_bytes([0; blake3::OUT_LEN])));

        self.write_metadata(path, metadata)?;

        Ok(())
    }
}

pub struct FileMeta {
    pub steps: Vec<ObjectPath>,
}
