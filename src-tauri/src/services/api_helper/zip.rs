//! The Linux release is a zip holding exactly one file: the helper binary.

use std::io::{Cursor, Read};

/// Cap on the unpacked size, so a hostile archive can't balloon in memory.
const MAX_UNPACKED_BYTES: u64 = 128 * 1024 * 1024;

pub fn extract_single_entry(zip: &[u8]) -> Result<Vec<u8>, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).map_err(|e| format!("zip: {e}"))?;
    if archive.len() != 1 {
        return Err(format!(
            "zip: expected exactly 1 entry, got {}",
            archive.len()
        ));
    }
    let entry = archive.by_index(0).map_err(|e| format!("zip: {e}"))?;
    let mut out = Vec::new();
    entry
        .take(MAX_UNPACKED_BYTES + 1)
        .read_to_end(&mut out)
        .map_err(|e| format!("zip: {e}"))?;
    if out.len() as u64 > MAX_UNPACKED_BYTES {
        return Err("zip: entry exceeds the size limit".into());
    }
    Ok(out)
}
