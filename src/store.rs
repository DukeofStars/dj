use std::path::PathBuf;

use base64::{engine::general_purpose::URL_SAFE, Engine};
use thiserror::Error;

use crate::{
    path::{ObjectPath, RepoPath},
    Repository,
};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to read file metadata for file: '{1}'")]
    FailedToReadFileMetadata(#[source] std::io::Error, PathBuf),
    #[error("Failed to write metadata file for file: '{1}")]
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

pub struct Store<'repo> {
    repo: &'repo Repository,
}

pub struct FileMeta {
    steps: Vec<ObjectPath>,
}

impl<'repo> Store<'repo> {
    pub fn new(repo: &'repo Repository) -> Store<'repo> {
        Store { repo }
    }

    pub fn store_path(&self) -> PathBuf {
        self.repo.path().join("store")
    }

    pub fn tracked_files(&self) -> Result<Vec<PathBuf>, Error> {
        let paths_dir = self.store_path().join("paths");
        Ok(paths_dir
            .read_dir()
            .map_err(|e| Error::FailedToReadStoreDir(e, paths_dir))?
            .into_iter()
            .filter_map(|entry| entry.ok())
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
    pub fn is_tracked(&self, path: &PathBuf) -> bool {
        let Some(path) = self.repo.relative_path(path) else {
            return false;
        };

        let metadata_path = self.get_metadata_path(&path);
        metadata_path.exists()
    }
    pub fn begin_tracking(&self, path: &PathBuf) -> Result<(), Error> {
        let path = self
            .repo
            .relative_path(path)
            .ok_or(Error::PathIsntInRepository(path.clone()))?;

        let metadata_path = self.get_metadata_path(&path);
        if !self.store_path().join("paths").exists() {
            std::fs::create_dir_all(self.store_path().join("paths"))
                .map_err(Error::FailedToCreateStoreDir)?;
        }
        if !metadata_path.exists() {
            std::fs::write(metadata_path, [])
                .map_err(|e| Error::FailedToWriteFileMetadata(e, path.clone()))?;
        }
        Ok(())
    }

    fn get_metadata_path(&self, path: &PathBuf) -> PathBuf {
        self.store_path()
            .join("paths")
            .join(URL_SAFE.encode(path.display().to_string().as_bytes()))
    }

    fn get_metadata(&self, path: &PathBuf) -> Result<FileMeta, Error> {
        let meta_path = self.get_metadata_path(path);
        let bytes = std::fs::read(&meta_path)
            .map_err(|e| Error::FailedToReadFileMetadata(e, path.clone()))?;
        let meta = parse_metadata(bytes.into_iter());
        Ok(meta)
    }

    fn add_step(&self, path: &PathBuf, step: ObjectPath) -> Result<RepoPath, Error> {
        let mut meta = self.get_metadata(path)?;
        meta.steps.push(step);

        let step = meta.steps.len() as u64 - 1;

        let metadata_path = self.get_metadata_path(path);
        if !metadata_path.parent().unwrap().exists() {
            std::fs::create_dir_all(&metadata_path.parent().unwrap())
                .map_err(Error::FailedToCreateStoreDir)?;
        }
        std::fs::write(metadata_path, write_metadata(meta))
            .map_err(|e| Error::FailedToWriteFileMetadata(e, path.clone()))?;

        Ok(RepoPath::new(step, self.repo.relative_path(path).unwrap()).unwrap())
    }

    pub fn generate_step(&self, path: &PathBuf) -> Result<ObjectPath, Error> {
        let path = self
            .repo
            .relative_path(path)
            .ok_or(Error::PathIsntInRepository(path.clone()))?;

        let hash =
            sha256::try_digest(&path).map_err(|e| Error::FailedToReadFile(e, path.clone()))?;

        if !self.store_path().join("objects").exists() {
            std::fs::create_dir_all(self.store_path().join("objects"))
                .map_err(Error::FailedToCreateStoreDir)?;
        }
        let obj_path = self.store_path().join("objects").join(&hash);
        std::fs::copy(&path, &obj_path)
            .map_err(|e| Error::FailedToCopyFile(e, path.clone(), obj_path.clone()))?;

        let (p1, p2) = hash.split_at(hash.len() / 2);
        let p1 = u128::from_str_radix(p1, 16).unwrap();
        let p2 = u128::from_str_radix(p2, 16).unwrap();
        let object_path = ObjectPath { p1, p2 };

        self.add_step(&path, object_path.clone())?;

        Ok(object_path)
    }

    // Read a object from the store.
    pub fn read(&self, path: impl AsRef<RepoPath>) -> Result<Vec<u8>, Error> {
        let path = path.as_ref();

        let meta = self.get_metadata(path.relative_path())?;
        let obj_path = meta
            .steps
            .get(*path.step() as usize)
            .ok_or(Error::PathDoesntExist(path.clone()))?;

        let path = self.store_path().join("objects").join(obj_path.to_string());

        Ok(std::fs::read(path).map_err(|e| Error::FailedToReadObject(e, obj_path.clone()))?)
    }

    // Write an object to the store.
    pub fn write<C: AsRef<[u8]>>(
        &self,
        path: impl AsRef<RepoPath>,
        content: C,
    ) -> Result<(), Error> {
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
    let mut next_chunk = || {
        let mut buf1 = [0; 16];
        let mut buf2 = [0; 16];

        for i in 0..16 {
            buf1[i] = bytes.next()?;
        }
        for i in 0..16 {
            buf2[i] = bytes.next()?;
        }

        Some((buf1, buf2))
    };

    let mut steps = Vec::new();

    while let Some((chunk1, chunk2)) = next_chunk() {
        let p1 = u128::from_be_bytes(chunk1);
        let p2 = u128::from_be_bytes(chunk2);
        let obj_path = ObjectPath { p1, p2 };
        steps.push(obj_path);
    }

    FileMeta { steps }
}
fn write_metadata(meta: FileMeta) -> Vec<u8> {
    meta.steps
        .into_iter()
        .map(|step| [step.p1.to_be_bytes(), step.p2.to_be_bytes()])
        .flatten()
        .flatten()
        .collect()
}
