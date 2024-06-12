use std::{fs::File, io::BufReader, path::PathBuf};

use base64::{engine::general_purpose::URL_SAFE, Engine};
use blake3::Hash;
use thiserror::Error;

use crate::{
    path::{ObjectPath, RepoPath},
    Repository,
};

use super::{FileMeta, Store};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to read file metadata for file: '{1}'")]
    FailedToReadFileMetadata(#[source] std::io::Error, PathBuf),
    #[error("Failed to write metadata file for file: '{1}'")]
    FailedToWriteFileMetadata(#[source] std::io::Error, PathBuf),
    #[error("Path '{0}' does not exist")]
    PathDoesntExist(RepoPath),
    #[error("Failed to read object: '{1}'")]
    FailedToReadObject(#[source] std::io::Error, ObjectPath),
    #[error("Failed to write to object: '{1}'")]
    FailedToWriteToObject(#[source] std::io::Error, ObjectPath),
    #[error("Failed to create store directory")]
    FailedToCreateStoreDir(#[source] std::io::Error),
    #[error("Failed to read file: '{1}'")]
    FailedToReadFile(#[source] std::io::Error, PathBuf),
    #[error("Failed to read a store directory: '{1}'")]
    FailedToReadStoreDir(#[source] std::io::Error, PathBuf),
    #[error("Failed to copy file '{1}' to '{2}'")]
    FailedToCopyFile(#[source] std::io::Error, PathBuf, PathBuf),
    #[error("Path isn't in repository: '{0}'")]
    PathIsntInRepository(PathBuf),
}

pub struct FileStore<'repo> {
    repo: &'repo Repository,
}

impl<'repo> FileStore<'repo> {
    pub fn new(repo: &'repo Repository) -> FileStore<'repo> {
        FileStore { repo }
    }

    pub fn store_path(&self) -> PathBuf {
        self.repo.path().join("store")
    }

    fn get_metadata_path(&self, path: &PathBuf) -> PathBuf {
        self.store_path()
            .join("paths")
            .join(self.repo.branch())
            .join(URL_SAFE.encode(path.display().to_string().as_bytes()))
    }
}
impl<'repo> Store for FileStore<'repo> {
    type Error = Error;

    fn tracked_files(&self) -> Result<Vec<PathBuf>, Self::Error> {
        let paths_dir = self.store_path().join("paths").join(self.repo.branch());
        Ok(paths_dir
            .read_dir()
            .map_err(|e| Error::FailedToReadStoreDir(e, paths_dir))?
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
            .map(|entry| {
                let path = entry.path();
                let file_name = path.file_name().unwrap().to_str().unwrap();
                URL_SAFE.decode(file_name).unwrap()
            })
            .map(String::from_utf8)
            .filter_map(|x| x.ok())
            .map(PathBuf::from)
            .collect())
    }

    fn is_tracked(&self, path: &PathBuf) -> bool {
        let Some(path) = self.repo.relative_path(path) else {
            return false;
        };

        let metadata_path = self.get_metadata_path(&path);
        metadata_path.exists()
    }

    fn begin_tracking(&self, path: &PathBuf) -> Result<(), Self::Error> {
        let path = self
            .repo
            .relative_path(path)
            .ok_or(Error::PathIsntInRepository(path.clone()))?;

        let metadata_path = self.get_metadata_path(&path);
        if !metadata_path.parent().unwrap().exists() {
            std::fs::create_dir_all(&metadata_path.parent().unwrap())
                .map_err(Error::FailedToCreateStoreDir)?;
        }
        if !metadata_path.exists() {
            std::fs::write(metadata_path, [])
                .map_err(|e| Error::FailedToWriteFileMetadata(e, path.clone()))?;
        }
        Ok(())
    }

    fn get_metadata(&self, path: &PathBuf) -> Result<FileMeta, Self::Error> {
        let meta_path = self.get_metadata_path(path);
        let bytes = std::fs::read(&meta_path)
            .map_err(|e| Error::FailedToReadFileMetadata(e, path.clone()))?;
        let meta = parse_metadata(bytes.into_iter());
        Ok(meta)
    }

    fn write_metadata(&self, path: &PathBuf, meta: FileMeta) -> Result<(), Self::Error> {
        let metadata_path = self.get_metadata_path(path);
        if !metadata_path.parent().unwrap().exists() {
            std::fs::create_dir_all(&metadata_path.parent().unwrap())
                .map_err(Error::FailedToCreateStoreDir)?;
        }

        let bytes = write_metadata(meta);
        std::fs::write(metadata_path, bytes)
            .map_err(|e| Error::FailedToWriteFileMetadata(e, path.clone()))
    }

    fn generate_step(&self, path: &PathBuf) -> Result<ObjectPath, Self::Error> {
        let path = self
            .repo
            .relative_path(path)
            .ok_or(Error::PathIsntInRepository(path.clone()))?;

        let mut hasher = blake3::Hasher::new();
        let file = File::open(&path).map_err(|e| Error::FailedToReadFile(e, path.clone()))?;
        let mut reader = BufReader::new(file);

        std::io::copy(&mut reader, &mut hasher)
            .map_err(|e| Error::FailedToReadFile(e, path.clone()))?;

        let hash = hasher.finalize();

        if !self.store_path().join("objects").exists() {
            std::fs::create_dir_all(self.store_path().join("objects"))
                .map_err(Error::FailedToCreateStoreDir)?;
        }
        let obj_path = self.store_path().join("objects").join(hash.to_string());
        std::fs::copy(&path, &obj_path)
            .map_err(|e| Error::FailedToCopyFile(e, path.clone(), obj_path.clone()))?;

        let object_path = ObjectPath(hash);
        self.add_step_to_metadata(&path, object_path.clone())?;

        Ok(object_path)
    }

    /// Read a object from the store.
    fn read(&self, path: impl AsRef<RepoPath>) -> Result<Vec<u8>, Self::Error> {
        let path = path.as_ref();

        let meta = self.get_metadata(path.relative_path())?;
        let obj_path = meta
            .steps
            .get(*path.step() as usize)
            .ok_or(Error::PathDoesntExist(path.clone()))?;

        let path = self.store_path().join("objects").join(obj_path.to_string());

        Ok(std::fs::read(path).map_err(|e| Error::FailedToReadObject(e, obj_path.clone()))?)
    }

    /// Write an object to the store.
    fn write<C: AsRef<[u8]>>(
        &self,
        path: impl AsRef<RepoPath>,
        content: C,
    ) -> Result<(), Self::Error> {
        let path = path.as_ref();

        let meta = self.get_metadata(path.relative_path())?;
        let obj_path = meta
            .steps
            .get(*path.step() as usize)
            .ok_or(Error::PathDoesntExist(path.clone()))?;
        let path = self.store_path().join("objects").join(obj_path.to_string());

        Ok(std::fs::write(path, content)
            .map_err(|e| Error::FailedToWriteToObject(e, obj_path.clone()))?)
    }
}

fn parse_metadata(mut bytes: impl Iterator<Item = u8>) -> FileMeta {
    let mut steps = Vec::new();

    let mut next_chunk = || -> Option<[u8; blake3::OUT_LEN]> {
        let mut out = [0; blake3::OUT_LEN];

        for i in 0..blake3::OUT_LEN {
            out[i] = bytes.next()?;
        }

        Some(out)
    };

    while let Some(bytes) = next_chunk() {
        let hash = Hash::from_bytes(bytes);
        let obj_path = ObjectPath(hash);
        steps.push(obj_path);
    }

    FileMeta { steps }
}
fn write_metadata(meta: FileMeta) -> Vec<u8> {
    meta.steps
        .into_iter()
        .map(|step| step.0.as_bytes().clone())
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use blake3::Hash;

    use crate::{path::ObjectPath, store::FileMeta};

    use super::write_metadata;

    #[test]
    fn assert_metadata_format_remains_same() {
        // The content of the data does not matter. We are just checking that the output remains the same.
        // Last updated: 13/06/2024
        let data: Vec<u8> = vec![
            18, 52, 86, 120, 144, 18, 52, 86, 120, 144, 18, 52, 86, 120, 144, 18, 52, 86, 120, 144,
            18, 52, 86, 120, 144, 18, 52, 86, 120, 144, 18, 52, 9, 135, 101, 67, 33, 9, 135, 101,
            67, 33, 9, 135, 101, 67, 33, 9, 135, 101, 67, 33, 9, 135, 101, 67, 33, 9, 135, 101, 67,
            33, 9, 135,
        ];

        let meta = FileMeta {
            steps: vec![
                ObjectPath(
                    Hash::from_hex(
                        b"1234567890123456789012345678901234567890123456789012345678901234",
                    )
                    .unwrap(),
                ),
                ObjectPath(
                    Hash::from_hex(
                        b"0987654321098765432109876543210987654321098765432109876543210987",
                    )
                    .unwrap(),
                ),
            ],
        };

        let new_data = write_metadata(meta);

        assert_eq!(data, new_data);
    }
}
