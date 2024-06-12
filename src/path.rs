use std::{fmt::Display, path::PathBuf, str::FromStr};

use blake3::Hash;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("The path is required to be relative, but it was not.")]
    PathIsntRelative(PathBuf),
    #[error("Failed to parse '{1}' as int.")]
    ParseIntError(#[source] core::num::ParseIntError, String),
    #[error("Failed to decode base64")]
    DecodeError(#[from] base64::DecodeError),
    #[error("Invalid utf8")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
    #[error("Invalid path syntax. (correct usage: '{{gen}}:{{path}}@{{step}}')")]
    InvalidPathSyntax,
}

#[derive(Debug, Clone)]
pub struct ObjectPath(pub(crate) Hash);
impl Display for ObjectPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        self.0.fmt(f)
    }
}
impl ObjectPath {
    pub fn hash(&self) -> &Hash {
        &self.0
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RepoPath {
    step: u64,
    relative_path: PathBuf,
    branch: String,
}

impl AsRef<RepoPath> for RepoPath {
    fn as_ref(&self) -> &RepoPath {
        &self
    }
}
impl Display for RepoPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}@{}",
            self.branch,
            self.relative_path.display(),
            self.step,
        )
    }
}
impl FromStr for RepoPath {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (branch, rest) = match s.split_once(":") {
            Some((left, right)) => (left, right),
            None => return Err(Error::InvalidPathSyntax),
        };
        let (path, step) = match rest.rsplit_once("@") {
            Some((left, right)) => (left, right),
            None => return Err(Error::InvalidPathSyntax),
        };

        let branch = branch.to_string();
        let step = u64::from_str(step).map_err(|e| Error::ParseIntError(e, step.to_string()))?;
        let path = PathBuf::from(path);

        RepoPath::new(branch, step, path)
    }
}

impl RepoPath {
    pub fn new(branch: String, step: u64, relative_path: PathBuf) -> Result<RepoPath, Error> {
        if !relative_path.is_relative() {
            return Err(Error::PathIsntRelative(relative_path));
        }

        Ok(Self {
            branch,
            step,
            relative_path,
        })
    }

    pub fn relative_path(&self) -> &PathBuf {
        &self.relative_path
    }
    pub fn step(&self) -> &u64 {
        &self.step
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use crate::path::{Error, RepoPath};

    #[test]
    pub fn from_str_test() {
        assert_eq!(
            RepoPath::from_str("local/main:src/main.rs@18").unwrap(),
            RepoPath::new("local/main".to_string(), 18, PathBuf::from("src/main.rs")).unwrap()
        );
    }

    #[test]
    pub fn to_str_test() {
        assert_eq!(
            RepoPath::new("local/main".to_string(), 18, PathBuf::from("src/main.rs"))
                .unwrap()
                .to_string(),
            "local/main:src/main.rs@18".to_string()
        )
    }

    #[test]
    pub fn round_trip_test() {
        let path =
            RepoPath::new("local/main".to_string(), 78, PathBuf::from("Cargo.toml")).unwrap();
        let text = path.to_string();
        let new_path = RepoPath::from_str(&text).unwrap();

        assert_eq!(path, new_path);
    }

    #[test]
    pub fn deny_non_relative_path_test() {
        #[cfg(not(windows))]
        let path_buf = PathBuf::from("/home/user/folder/file.txt");
        #[cfg(windows)]
        let path_buf = PathBuf::from(r#"C:\Users\user\folder\file.txt"#);

        let res = RepoPath::new("local/main".to_string(), 2, path_buf.clone());
        let err = res.unwrap_err();
        assert_eq!(err, Error::PathIsntRelative(path_buf))
    }
}
