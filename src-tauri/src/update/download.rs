//! Fetching an offered release into PADE's updates folder and checking it against its digest.

use super::manifest::Offer;
use super::{updates_directory, user_agent};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

const DOWNLOAD_SUFFIX: &str = ".part";

const HASH_CHUNK_BYTES: usize = 64 * 1024;

/// The SHA-256 of a file, as lowercase hex, read in chunks.
fn file_sha256(path: &Path) -> std::io::Result<String> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; HASH_CHUNK_BYTES];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(crate::util::lowercase_hex(&hasher.finalize()))
}

pub(super) fn is_digest_of(path: &Path, sha256: &str) -> bool {
    file_sha256(path).is_ok_and(|actual| actual == sha256)
}

/// Fetch the offered file into PADE's updates folder as `file_name`, checked
/// against its digest before it is kept. A download from an earlier run that
/// still matches is used as it is; other leftovers are removed.
pub(super) fn download(offer: &Offer, file_name: &str) -> Result<PathBuf, String> {
    let directory = updates_directory().ok_or("no local data folder")?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let target = directory.join(file_name);
    if is_digest_of(&target, &offer.sha256) {
        return Ok(target);
    }
    // Several PADE windows can run as separate processes; each downloads into
    // its own partial file so two never write one.
    let partial = directory.join(format!(
        "{file_name}{DOWNLOAD_SUFFIX}-{}",
        std::process::id()
    ));
    let response = ureq::get(&offer.url)
        .header("User-Agent", &user_agent())
        .call()
        .map_err(|error| error.to_string())?;
    let mut reader = response.into_body().into_reader();
    let mut destination = File::create(&partial).map_err(|error| error.to_string())?;
    std::io::copy(&mut reader, &mut destination).map_err(|error| error.to_string())?;
    drop(destination);
    if !is_digest_of(&partial, &offer.sha256) {
        let _ = std::fs::remove_file(&partial);
        return Err("the downloaded update does not match its digest".into());
    }
    std::fs::rename(&partial, &target).map_err(|error| error.to_string())?;
    delete_other_downloads(&directory, &target);
    Ok(target)
}

fn delete_other_downloads(directory: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path != keep {
            // A file another PADE is still writing stays; it is cleaned next time.
            let _ = std::fs::remove_file(path);
        }
    }
}
