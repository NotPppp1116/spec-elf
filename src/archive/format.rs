use crate::arch::x86::{X86Level, detect_x86_level, native_hasher};
use std::{
    fs::{File, OpenOptions},
    io::{self, Cursor, Error, ErrorKind, Read, Seek, SeekFrom, Write},
    path::Path,
};

/// Magic bytes stored at the start of the footer.
///
/// Because the footer is written last, this lets the launcher quickly check
/// whether the current executable is a packed spec-elf archive.
const FOOTER_MAGIC: &[u8; 8] = b"VPKFOOT\0";

/// Fixed footer size in bytes:
///
/// - 8 bytes magic
/// - 8 bytes manifest offset
/// - 8 bytes manifest size
/// - 8 bytes native CPU hash
/// - 1 byte launch flag
const FOOTER_SIZE: u64 = 33;

/// Footer flag that marks the file as a launchable archive.
const IS_LAUNCHED: u8 = 1;

/// A single packed payload entry stored in the manifest.
///
/// `offset` and `size` describe the byte range of one compiled executable
/// inside the packed launcher file.
struct Entry {
    name: String,
    offset: u64,
    size: u64,
}

const COMPAT_X86_64: [X86Level; 1] = [X86Level::X86_64];
const COMPAT_V2: [X86Level; 2] = [X86Level::V2, X86Level::X86_64];
const COMPAT_V3: [X86Level; 3] = [X86Level::V3, X86Level::V2, X86Level::X86_64];
const COMPAT_V4: [X86Level; 4] = [X86Level::V4, X86Level::V3, X86Level::V2, X86Level::X86_64];

/// Read a little-endian `u32` from the current file position.
fn read_u32(file: &mut File) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    file.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

/// Read a little-endian `u64` from the current file position.
fn read_u64(file: &mut File) -> io::Result<u64> {
    let mut buf = [0u8; 8];
    file.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

/// Build a packed executable by appending payloads, a manifest, and a footer.
///
/// The resulting file still starts with the original launcher bytes, so the OS
/// can execute it normally. Everything after the launcher is data that the
/// launcher reads back from its own executable at runtime.
pub fn pack_files<P, O>(
    launcher_path: P,
    output_path: O,
    payload_paths: &[String],
) -> io::Result<()>
where
    P: AsRef<Path>,
    O: AsRef<Path>,
{
    let launcher_path = launcher_path.as_ref();
    let output_path = output_path.as_ref();

    let mut output = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(output_path)?;

    // Copy the launcher first. This preserves a valid executable header at the
    // beginning of the packed file.
    let mut launcher = File::open(launcher_path)?;
    io::copy(&mut launcher, &mut output)?;

    // Keep the packed file executable when the launcher was executable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = std::fs::metadata(launcher_path)?.permissions().mode();
        output.set_permissions(std::fs::Permissions::from_mode(mode))?;
    }

    let mut entries = Vec::with_capacity(payload_paths.len());

    for payload_path in payload_paths {
        let payload_path = Path::new(payload_path);

        // The current output position is the start of this payload inside the
        // final packed file. The manifest stores this absolute offset.
        let offset = output.stream_position()?;
        let mut payload = File::open(payload_path).map_err(|err| {
            Error::new(
                err.kind(),
                format!("failed to open payload {}: {err}", payload_path.display()),
            )
        })?;

        let payload = zstd::encode_all(&mut payload, 19)?;

        let size = io::copy(&mut Cursor::new(payload), &mut output)?;
        let name = payload_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidInput,
                    "payload path has no valid file name",
                )
            })?
            .to_string();

        entries.push(Entry { name, offset, size });
    }

    // The manifest is appended after the launcher and all payload blobs. It is
    // read from the offset stored in the footer.
    let manifest_offset = output.stream_position()?;
    output.write_all(&(entries.len() as u32).to_le_bytes())?;

    for entry in &entries {
        let name_bytes = entry.name.as_bytes();
        output.write_all(&(name_bytes.len() as u32).to_le_bytes())?;
        output.write_all(name_bytes)?;
        output.write_all(&entry.offset.to_le_bytes())?;
        output.write_all(&entry.size.to_le_bytes())?;
    }

    let manifest_size = output.stream_position()? - manifest_offset;

    // The footer is fixed-size and always last, which lets the reader locate it
    // with one seek from the end of the file.
    output.write_all(FOOTER_MAGIC)?;
    output.write_all(&manifest_offset.to_le_bytes())?;
    output.write_all(&manifest_size.to_le_bytes())?;

    let native_hash = native_hasher();

    match native_hash {
        Some(v) => output.write_all(&v.to_le_bytes())?,
        None => output.write_all(&0u64.to_le_bytes())?,
    }

    output.write_all(&[IS_LAUNCHED])?;

    Ok(())
}

/// Read the packed file footer, locate the best matching payload, and return it.
pub fn read_back<P>(path: P) -> io::Result<Vec<u8>>
where
    P: AsRef<Path>,
{
    let mut file = OpenOptions::new().read(true).open(path)?;

    let file_size = file.metadata()?.len();

    if file_size < FOOTER_SIZE {
        return Err(Error::new(ErrorKind::InvalidData, "file too small"));
    }

    file.seek(SeekFrom::End(-(FOOTER_SIZE as i64)))?;

    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)?;

    if &magic != FOOTER_MAGIC {
        return Err(Error::new(ErrorKind::InvalidData, "invalid footer magic"));
    }

    // Footer layout: magic, manifest offset, manifest size, native hash, launch flag.
    let manifest_offset = read_u64(&mut file)?;
    let manifest_size = read_u64(&mut file)?;

    let manifest_end = manifest_offset
        .checked_add(manifest_size)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "manifest range overflow"))?;

    if manifest_end > file_size - FOOTER_SIZE {
        return Err(Error::new(ErrorKind::InvalidData, "invalid manifest range"));
    }

    file.seek(SeekFrom::Start(manifest_offset))?;

    let entry_count = read_u32(&mut file)?;

    // Each manifest entry stores the payload name and its byte range.
    let mut entries = Vec::with_capacity(entry_count as usize);

    for _ in 0..entry_count {
        let name_len = read_u32(&mut file)? as usize;

        let mut name_bytes = vec![0u8; name_len];
        file.read_exact(&mut name_bytes)?;

        let name = String::from_utf8(name_bytes)
            .map_err(|_| Error::new(ErrorKind::InvalidData, "invalid UTF-8 in file name"))?;

        let offset = read_u64(&mut file)?;
        let size = read_u64(&mut file)?;
        let payload_end = offset
            .checked_add(size)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "payload range overflow"))?;

        if payload_end > manifest_offset {
            return Err(Error::new(ErrorKind::InvalidData, "invalid payload range"));
        }

        entries.push(Entry { name, offset, size });
    }

    // The native hash is the 8-byte field before the final launch flag.
    file.seek(SeekFrom::End(-9))?;
    let mut native_hash = [0u8; 8];
    file.read_exact(&mut native_hash)?;

    let (offset, size) = find_optimal(&entries, &native_hash)?;
    let size = usize::try_from(size)
        .map_err(|_| Error::new(ErrorKind::InvalidData, "payload too large"))?;

    let mut correct_exe = vec![0u8; size];

    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut correct_exe)?;

    let correct_exe = zstd::decode_all(Cursor::new(correct_exe))?;

    Ok(correct_exe)
}

/// Pick the payload that best matches the current machine.
fn find_optimal(entries: &[Entry], native_hash: &[u8]) -> io::Result<(u64, u64)> {
    let level = detect_x86_level();

    // Prefer the `native` build only when the CPU/platform hash written during
    // packing matches the current machine. Otherwise, fall back to the portable
    // x86-64 level names.
    if let Some(b) = native_hasher()
        && b.to_le_bytes() == native_hash
    {
        for entry in entries {
            if entry.name.contains("native") {
                return Ok((entry.offset, entry.size));
            }
        }
    }

    if let Some(entry) = find_best_portable(entries, level) {
        return Ok((entry.offset, entry.size));
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no compatible binary found",
    ))
}

fn find_best_portable(entries: &[Entry], current_level: X86Level) -> Option<&Entry> {
    for level in compatible_levels(current_level) {
        if let Some(entry) = entries
            .iter()
            .find(|entry| entry_matches_level(entry, *level))
        {
            return Some(entry);
        }
    }

    None
}

fn compatible_levels(current_level: X86Level) -> &'static [X86Level] {
    match current_level {
        X86Level::X86_64 => &COMPAT_X86_64,
        X86Level::V2 => &COMPAT_V2,
        X86Level::V3 => &COMPAT_V3,
        X86Level::V4 => &COMPAT_V4,
    }
}

fn entry_matches_level(entry: &Entry, level: X86Level) -> bool {
    let (dash_name, underscore_name) = level_names(level);

    entry.name == dash_name
        || entry.name.ends_with(dash_name)
        || entry.name == underscore_name
        || entry.name.ends_with(underscore_name)
        || entry.name.strip_prefix("-march=") == Some(dash_name)
        || entry.name.strip_prefix("-mcpu=") == Some(underscore_name)
}

fn level_names(level: X86Level) -> (&'static str, &'static str) {
    match level {
        X86Level::V4 => ("x86-64-v4", "x86_64_v4"),
        X86Level::V3 => ("x86-64-v3", "x86_64_v3"),
        X86Level::V2 => ("x86-64-v2", "x86_64_v2"),
        X86Level::X86_64 => ("x86-64", "x86_64"),
    }
}

/// Return whether a file looks like a launchable spec-elf archive.
pub fn is_archive<P>(path: P) -> io::Result<bool>
where
    P: AsRef<Path>,
{
    let mut file = OpenOptions::new().read(true).open(path)?;

    let file_size = file.metadata()?.len();

    if file_size < FOOTER_SIZE {
        return Ok(false);
    }

    file.seek(SeekFrom::End(-(FOOTER_SIZE as i64)))?;

    let mut identifier = [0u8; 8];
    file.read_exact(&mut identifier)?;

    // A matching magic value means the footer is present. The final byte is a
    // separate launch flag so future format versions can distinguish packed
    // data from a runnable launcher archive.
    if &identifier == FOOTER_MAGIC {
        file.seek(SeekFrom::End(-1))?;
        let mut is_launched = [0u8; 1];
        file.read_exact(&mut is_launched)?;

        if is_launched[0] == IS_LAUNCHED {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arch::x86::{X86Level, detect_x86_level, native_hasher};
    use std::fs;

    fn entry(name: &str, offset: u64) -> Entry {
        Entry {
            name: name.to_string(),
            offset,
            size: 1,
        }
    }

    #[test]
    fn normal_file_is_not_archive() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("plain-file");

        fs::write(&file, b"hello")?;

        assert!(!is_archive(&file)?);

        Ok(())
    }

    #[test]
    fn fallback_uses_best_lower_compatible_level() {
        let entries = vec![
            entry("c-x86-64", 1),
            entry("c-x86-64-v2", 2),
            entry("c-x86-64-v3", 3),
        ];

        let selected = find_best_portable(&entries, X86Level::V4).expect("compatible payload");

        assert_eq!(selected.offset, 3);
    }

    #[test]
    fn fallback_never_uses_higher_incompatible_level() {
        let entries = vec![entry("c-x86-64", 1), entry("c-x86-64-v4", 4)];

        let selected = find_best_portable(&entries, X86Level::V3).expect("compatible payload");

        assert_eq!(selected.offset, 1);
    }

    #[test]
    fn fallback_accepts_underscore_level_names() {
        let entries = vec![entry("rust-x86_64", 1), entry("rust-x86_64_v2", 2)];

        let selected = find_best_portable(&entries, X86Level::V3).expect("compatible payload");

        assert_eq!(selected.offset, 2);
    }

    #[test]
    fn packed_file_is_archive() -> io::Result<()> {
        let dir = tempfile::tempdir()?;

        let launcher = dir.path().join("launcher");
        let output = dir.path().join("packed");

        let native = dir.path().join("c-native");
        let x86_64 = dir.path().join("c-x86-64");
        let v2 = dir.path().join("c-x86-64-v2");
        let v3 = dir.path().join("c-x86-64-v3");
        let v4 = dir.path().join("c-x86-64-v4");

        fs::write(&launcher, b"fake launcher")?;
        fs::write(&native, b"native payload")?;
        fs::write(&x86_64, b"x86-64 payload")?;
        fs::write(&v2, b"x86-64-v2 payload")?;
        fs::write(&v3, b"x86-64-v3 payload")?;
        fs::write(&v4, b"x86-64-v4 payload")?;

        let payloads = vec![
            native.display().to_string(),
            x86_64.display().to_string(),
            v2.display().to_string(),
            v3.display().to_string(),
            v4.display().to_string(),
        ];

        pack_files(&launcher, &output, &payloads)?;

        assert!(is_archive(&output)?);

        Ok(())
    }

    #[test]
    fn packed_file_reads_best_payload() -> io::Result<()> {
        let dir = tempfile::tempdir()?;

        let launcher = dir.path().join("launcher");
        let output = dir.path().join("packed");

        let native = dir.path().join("c-native");
        let x86_64 = dir.path().join("c-x86-64");
        let v2 = dir.path().join("c-x86-64-v2");
        let v3 = dir.path().join("c-x86-64-v3");
        let v4 = dir.path().join("c-x86-64-v4");

        fs::write(&launcher, b"fake launcher")?;
        fs::write(&native, b"native payload")?;
        fs::write(&x86_64, b"x86-64 payload")?;
        fs::write(&v2, b"x86-64-v2 payload")?;
        fs::write(&v3, b"x86-64-v3 payload")?;
        fs::write(&v4, b"x86-64-v4 payload")?;

        let payloads = vec![
            native.display().to_string(),
            x86_64.display().to_string(),
            v2.display().to_string(),
            v3.display().to_string(),
            v4.display().to_string(),
        ];

        pack_files(&launcher, &output, &payloads)?;

        let actual = read_back(&output)?;

        let expected: &[u8] = if native_hasher().is_some() {
            b"native payload"
        } else {
            match detect_x86_level() {
                X86Level::X86_64 => b"x86-64 payload",
                X86Level::V2 => b"x86-64-v2 payload",
                X86Level::V3 => b"x86-64-v3 payload",
                X86Level::V4 => b"x86-64-v4 payload",
            }
        };

        assert_eq!(actual, expected);

        Ok(())
    }
    #[test]
    fn packed_file_accepts_non_utf8_binary_payloads() -> io::Result<()> {
        let dir = tempfile::tempdir()?;

        let launcher = dir.path().join("launcher");
        let output = dir.path().join("packed");

        let native = dir.path().join("c-native");
        let x86_64 = dir.path().join("c-x86-64");
        let v2 = dir.path().join("c-x86-64-v2");
        let v3 = dir.path().join("c-x86-64-v3");
        let v4 = dir.path().join("c-x86-64-v4");

        // This intentionally contains invalid UTF-8 bytes.
        // Old buggy code using read_to_string() will fail here.
        let binary_payload = b"\x7fELF\x02\x01\x01\x00\xff\xfe\xfd\x00binary payload";

        fs::write(&launcher, b"fake launcher")?;
        fs::write(&native, binary_payload)?;
        fs::write(&x86_64, binary_payload)?;
        fs::write(&v2, binary_payload)?;
        fs::write(&v3, binary_payload)?;
        fs::write(&v4, binary_payload)?;

        let payloads = vec![
            native.display().to_string(),
            x86_64.display().to_string(),
            v2.display().to_string(),
            v3.display().to_string(),
            v4.display().to_string(),
        ];

        pack_files(&launcher, &output, &payloads)?;

        let actual = read_back(&output)?;

        assert_eq!(actual, binary_payload);

        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn packed_file_preserves_launcher_executable_permissions() -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir()?;

        let launcher = dir.path().join("launcher");
        let output = dir.path().join("packed");
        let payload = dir.path().join("c-x86-64");

        fs::write(&launcher, b"fake launcher")?;
        fs::write(&payload, b"payload")?;

        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755))?;

        let payloads = vec![payload.display().to_string()];

        pack_files(&launcher, &output, &payloads)?;

        let mode = fs::metadata(&output)?.permissions().mode();

        assert_ne!(
            mode & 0o111,
            0,
            "packed launcher should keep at least one executable bit"
        );

        Ok(())
    }
}
