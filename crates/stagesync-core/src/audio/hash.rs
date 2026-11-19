//! Streaming file hashing (SHA-256), used to prove that originals were not modified.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Compute the SHA-256 of a file without loading it into memory.
///
/// The file is read through a 1 MiB buffer, so this is safe for multi-gigabyte recordings.
pub fn sha256_file(path: &Path) -> Result<String> {
    sha256_file_cancellable(path, None)
}

/// Preserve full-file integrity checks while allowing background preparation to stop.
pub fn sha256_file_cancellable(path: &Path, cancel: Option<&AtomicBool>) -> Result<String> {
    let file = File::open(path)
        .with_context(|| format!("failed to open file for hashing: {}", path.display()))?;
    let mut reader = BufReader::with_capacity(1 << 20, file);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            anyhow::bail!("audio preparation cancelled");
        }
        let n = reader
            .read(&mut buf)
            .with_context(|| format!("failed to read file for hashing: {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// SHA-256 of an in-memory byte slice (used for output determinism checks).
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
