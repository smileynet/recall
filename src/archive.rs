//! Shared archive extraction helpers.
//!
//! Both the ORT runtime download (`embed`) and the self-update binary download
//! (`update`) need to pull a single named file out of a `.zip`. This centralizes
//! that on the `zip` crate (deflate-only) instead of two hand-rolled parsers.

use std::io::{Cursor, Read};

use anyhow::{Context, Result};

/// Extract the first entry whose path ends with `wanted` from a zip archive in
/// memory, returning its bytes.
///
/// Matches by suffix (not `by_name`) because release/runtime archives nest the
/// target inside a versioned directory (e.g. `onnxruntime-win-x64-1.20.0/lib/
/// onnxruntime.dll`, or `recall-<triple>/recall.exe`). Directory entries and
/// non-matching files are skipped.
pub fn extract_named_from_zip(bytes: &[u8], wanted: &str) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("reading zip archive")?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).context("reading zip entry")?;
        if !entry.is_file() {
            continue;
        }
        // `name()` is the full in-archive path; match on its final component/suffix.
        let name = entry.name().to_string();
        let matches = std::path::Path::new(&name)
            .file_name()
            .and_then(|f| f.to_str())
            .map(|f| f == wanted || f.ends_with(wanted))
            .unwrap_or(false)
            || name.ends_with(wanted);
        if matches {
            let mut out = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut out)
                .with_context(|| format!("decompressing '{}' from zip", name))?;
            return Ok(out);
        }
    }
    anyhow::bail!("no entry matching '{}' found in zip archive", wanted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    /// Build a small in-memory zip containing `entries` (path, contents).
    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for (path, data) in entries {
                zw.start_file(*path, opts).unwrap();
                zw.write_all(data).unwrap();
            }
            zw.finish().unwrap();
        }
        buf
    }

    #[test]
    fn extracts_nested_named_file() {
        let zip = make_zip(&[
            ("pkg-1.2.3/lib/libthing.dll", b"DLLBYTES"),
            ("pkg-1.2.3/README", b"ignore me"),
        ]);
        let got = extract_named_from_zip(&zip, "libthing.dll").unwrap();
        assert_eq!(got, b"DLLBYTES");
    }

    #[test]
    fn extracts_top_level_named_file() {
        let zip = make_zip(&[("recall.exe", b"BINARY")]);
        let got = extract_named_from_zip(&zip, "recall.exe").unwrap();
        assert_eq!(got, b"BINARY");
    }

    #[test]
    fn missing_entry_errors() {
        let zip = make_zip(&[("something-else.txt", b"x")]);
        assert!(extract_named_from_zip(&zip, "recall.exe").is_err());
    }
}
