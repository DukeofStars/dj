use std::path::PathBuf;

use thiserror::Error;
use tracing::{debug, error, trace};

use crate::{Options, StorePath};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to read file '{}'", .1.display())]
    FailedToReadFile(#[source] std::io::Error, PathBuf),
    #[error("Failed to write file '{}'", .1.display())]
    FailedToWriteFile(#[source] std::io::Error, PathBuf),
    #[error("Store directory doesn't exist")]
    StoreDirDoesntExist,
}

type Result<T> = std::result::Result<T, Error>;

pub fn write_file_to_store(file: PathBuf, options: &Options) -> Result<StorePath> {
    debug!(file = %file.display(), "Writing file to store");

    let bytes = std::fs::read(&file).map_err(|e| Error::FailedToReadFile(e, file.clone()))?;
    if !options.disable_logging {
        trace!("Read {} bytes from '{}'", bytes.len(), file.display());
    }

    write_bytes_to_store(bytes, options)
}

pub fn write_bytes_to_store(bytes: Vec<u8>, options: &Options) -> Result<StorePath> {
    if !options.disable_logging {
        let display_bytes =
            String::from_utf8(bytes.clone()).unwrap_or_else(|_| format!("{bytes:?}"));
        debug!(bytes = %display_bytes, "Writing bytes to store");
    }

    let hash = blake3::hash(&bytes);
    trace!(%hash, "Computed hash");

    let store_path = StorePath { hash };

    write_bytes_to_store_path(bytes, &store_path, options)?;

    Ok(store_path)
}

pub fn write_bytes_to_store_path(
    bytes: Vec<u8>,
    store_path: &StorePath,
    options: &Options,
) -> Result<()> {
    let store_dir = options.repo_path.join("store");
    if !store_dir.exists() {
        error!("Store directory doesn't exist!");
        return Err(Error::StoreDirDoesntExist);
    }
    let path = options.repo_path.join("store").join(store_path.to_string());

    std::fs::write(&path, &bytes).map_err(|e| Error::FailedToWriteFile(e, path.clone()))?;
    trace!(
        "Wrote '{}' bytes to store path '{}'",
        bytes.len(),
        store_path.to_string()
    );

    Ok(())
}
