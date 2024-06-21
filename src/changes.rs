use std::{fmt::Display, path::PathBuf};

use blake3::Hash;
use prettydiff::{basic::SliceChangeset, text::LineChangeset};
use thiserror::Error;

use crate::{path::RepoPath, store::Store, Repository};

pub trait RepoState {
    type Error: std::error::Error;

    /// Return all paths that exist in the repo state, relative to it's root.
    fn files(&self) -> Result<Vec<PathBuf>, Self::Error>;

    /// Read a file, returning it's bytes
    /// It is assumed that this is some IO operation, so calls to this should be minimised.
    fn read(&self, path: &PathBuf) -> Result<Vec<u8>, Self::Error>;

    /// Return the hash of a file.
    fn hash(&self, path: &PathBuf) -> Result<Hash, Self::Error> {
        let contents = self.read(path)?;
        let hash = blake3::hash(&contents);
        Ok(hash)
    }
}
pub struct Diff {
    pub new_files: Vec<(PathBuf, Vec<u8>)>,
    pub changed_files: Vec<(PathBuf, FileDiff<'static>)>,
    pub deleted_files: Vec<PathBuf>,
}
pub enum FileDiff<'a> {
    ByLine(LineChangeset<'a>),
    ByBytes(SliceChangeset<'a, u8>),
}
impl<'a> Display for FileDiff<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileDiff::ByLine(c) => c.fmt(f),
            FileDiff::ByBytes(c) => c.fmt(f),
        }
    }
}

pub struct RepoStateFromWorking<'repo> {
    repo: &'repo Repository,
}
impl<'repo> RepoState for RepoStateFromWorking<'repo> {
    type Error = std::io::Error;

    fn files(&self) -> Result<Vec<PathBuf>, Self::Error> {
        fn iter_dir(p: &PathBuf) -> Result<Vec<PathBuf>, std::io::Error> {
            let mut files = Vec::new();
            for file in p.read_dir()? {
                let file = &file?;
                let file_type = file.file_type()?;
                if file_type.is_file() {
                    files.push(file.path());
                } else if file_type.is_dir() {
                    files.append(&mut iter_dir(&file.path())?)
                }
            }

            Ok(files)
        }

        let files = iter_dir(self.repo.work_dir())?;
        Ok(files)
    }

    fn read(&self, path: &PathBuf) -> Result<Vec<u8>, Self::Error> {
        let path = self.repo.work_dir().join(path);
        let bytes = std::fs::read(path)?;

        Ok(bytes)
    }
}
impl<'repo> RepoStateFromWorking<'repo> {
    pub fn new(repo: &'repo Repository) -> Self {
        Self { repo }
    }
}

#[derive(Debug, Error)]
pub enum RepoStateFromStoreError<S: Store> {
    #[error("Error occured with the store")]
    StoreError(S::Error),
    #[error("File {} doesn't exist", .0.display())]
    FileDoesntExist(PathBuf),
}
pub struct RepoStateFromStore<'store, S: Store> {
    store: &'store S,
    objects: Vec<RepoPath>,
}
impl<'store, S: Store + std::fmt::Debug> RepoState for RepoStateFromStore<'store, S> {
    type Error = RepoStateFromStoreError<S>;

    fn files(&self) -> Result<Vec<PathBuf>, Self::Error> {
        Ok(self
            .objects
            .iter()
            .map(|p| p.relative_path().clone())
            .collect())
    }

    fn read(&self, path: &PathBuf) -> Result<Vec<u8>, Self::Error> {
        let repo_path = self
            .objects
            .iter()
            .filter(|p| p.relative_path() == path)
            .next()
            .ok_or(RepoStateFromStoreError::FileDoesntExist(path.clone()))?;
        self.store
            .read(repo_path)
            .map_err(RepoStateFromStoreError::StoreError)
    }
}
impl<'store, S: Store> RepoStateFromStore<'store, S> {
    pub fn from_latest(store: &'store S) -> Result<RepoStateFromStore<'store, S>, S::Error> {
        let objects = store
            .tracked_files()?
            .into_iter()
            .map(|p| Ok((store.get_metadata(&p)?, p)))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|(meta, _)| meta.steps.len() > 0)
            .map(|(meta, p)| (meta.steps.len(), p))
            .map(|(step, path)| RepoPath::new(None, step as u64, path).unwrap())
            .collect();

        Ok(RepoStateFromStore { store, objects })
    }
}

#[derive(Debug, Error)]
pub enum CompareError<E1, E2> {
    #[error(transparent)]
    State1Error(E1),
    #[error(transparent)]
    State2Error(E2),
}

// This is just a naive implementation
pub fn compare_states<'a, S: Store, T: RepoState, U: RepoState>(
    state1: T,
    state2: U,
) -> Result<Diff, CompareError<T::Error, U::Error>> {
    let files1 = state1.files().map_err(CompareError::State1Error)?;
    let files2 = state2.files().map_err(CompareError::State2Error)?;

    let new_files = files2
        .iter()
        .filter(|f| !files1.contains(f))
        .map(|p| {
            Ok((
                p.clone(),
                state2.read(p).map_err(CompareError::State2Error)?,
            ))
        })
        .collect::<Result<_, _>>()?;
    let changed_files = files2
        .iter()
        // Only get files that both states contain
        .filter(|f| files1.contains(f))
        .filter_map(|p| {
            // Get the old hash, returning an error if it occurs.
            let old_hash = match state1.hash(p).map_err(CompareError::State1Error) {
                Ok(hash) => hash,
                Err(err) => return Some(Err(err)),
            };
            // Get the new hash, returning an error if it occurs.
            let new_hash = match state2.hash(p).map_err(CompareError::State2Error) {
                Ok(hash) => hash,
                Err(err) => return Some(Err(err)),
            };

            let changed = old_hash != new_hash;

            if changed {
                Some(Ok(p))
            } else {
                // If it has not changed, return none, so it is ignored from the list
                None
            }
        })
        .collect::<Result<Vec<&PathBuf>, CompareError<_, _>>>()?
        .into_iter()
        .map(|p| {
            let bytes1 = Box::new(
                state1
                    .read(&p)
                    .map_err(CompareError::<_, U::Error>::State1Error)?,
            );
            let bytes2 = Box::new(
                state2
                    .read(&p)
                    .map_err(CompareError::<T::Error, _>::State2Error)?,
            );

            let bytes1 = Box::leak(bytes1);
            let bytes2 = Box::leak(bytes2);

            let diff = file_diff(p, bytes1, bytes2);

            Ok((p.clone(), diff))
        })
        .collect::<Result<_, _>>()?;
    let deleted_files = files1
        .iter()
        .filter(|p| !files2.contains(p))
        .map(|p| p.clone())
        .collect();

    Ok(Diff {
        new_files,
        changed_files,
        deleted_files,
    })
}

fn file_diff<'a>(_path: &PathBuf, bytes1: &'a [u8], bytes2: &'a [u8]) -> FileDiff<'a> {
    FileDiff::ByBytes(prettydiff::diff_slice(&bytes1, &bytes2))
}
