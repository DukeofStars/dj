use std::{fmt::Display, path::PathBuf, str::FromStr};

use base64::{engine::general_purpose::URL_SAFE, Engine};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("The path is required to be relative, but it was not.")]
    PathIsntRelative(PathBuf),
    #[error("The path has no timestamp (e.g. _._@some/path)")]
    NoTimestamp,
    #[error("Failed to parse '{1}' as int.")]
    ParseIntError(#[source] core::num::ParseIntError, String),
    #[error("Failed to decode base64")]
    DecodeError(#[from] base64::DecodeError),
    #[error("Invalid utf8")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Path {
    generation: u64,
    step: u64,
    relative_path: PathBuf,
}

impl Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{}@{}",
            self.generation,
            self.step,
            self.relative_path.display()
        )
    }
}
impl FromStr for Path {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((timestamp, path)) = s.split_once("@") else {
            return Err(Error::NoTimestamp);
        };

        let path = PathBuf::from(path);

        let (gen, step) = match timestamp.split_once(".") {
            Some((left, right)) => (left, right),
            None => (timestamp, "0"),
        };
        let generation =
            u64::from_str(gen).map_err(|e| Error::ParseIntError(e, gen.to_string()))?;
        let step = u64::from_str(step).map_err(|e| Error::ParseIntError(e, step.to_string()))?;

        Path::new(generation, step, path)
    }
}

impl Path {
    pub fn new(generation: u64, step: u64, relative_path: PathBuf) -> Result<Path, Error> {
        if !relative_path.is_relative() {
            return Err(Error::PathIsntRelative(relative_path));
        }

        Ok(Self {
            generation,
            step,
            relative_path,
        })
    }

    pub fn to_store_path(&self) -> String {
        let text = self.to_string();
        let encoded = URL_SAFE.encode(&text);

        encoded
    }
    pub fn from_store_path(text: String) -> Result<Path, Error> {
        let decoded = URL_SAFE.decode(&text)?;
        let text = String::from_utf8(decoded)?;
        let path = Path::from_str(&text)?;

        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use crate::path::{Error, Path};

    #[test]
    pub fn from_str_test() {
        assert_eq!(
            Path::from_str("43.18@src/main.rs").unwrap(),
            Path::new(43, 18, PathBuf::from("src/main.rs")).unwrap()
        );
    }

    #[test]
    pub fn to_str_test() {
        assert_eq!(
            Path::new(43, 18, PathBuf::from("src/main.rs"))
                .unwrap()
                .to_string(),
            "43.18@src/main.rs".to_string()
        )
    }

    #[test]
    pub fn round_trip_test() {
        let path = Path::new(23, 78, PathBuf::from("Cargo.toml")).unwrap();
        let text = path.to_string();
        let new_path = Path::from_str(&text).unwrap();

        assert_eq!(path, new_path);
    }

    #[test]
    pub fn deny_non_relative_path_test() {
        #[cfg(not(windows))]
        let path_buf = PathBuf::from("/home/user/folder/file.txt");
        #[cfg(windows)]
        let path_buf = PathBuf::from(r#"C:\Users\user\folder\file.txt"#);
        let res = Path::new(12, 2, path_buf.clone());
        let err = res.unwrap_err();
        assert_eq!(err, Error::PathIsntRelative(path_buf))
    }
}
