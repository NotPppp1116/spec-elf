// src/bin/gen-fuzz-corpus.rs
use spec_elf::archive::format::pack_files;
use std::fs;
use std::io;
use std::path::PathBuf;

fn main() -> io::Result<()> {
    let root = PathBuf::from("fuzz/generated");
    let input = PathBuf::from("fuzz/in");

    fs::create_dir_all(&root)?;
    fs::create_dir_all(&input)?;

    let launcher = root.join("launcher");
    fs::write(&launcher, b"fake launcher bytes\n")?;

    let payloads = [
        ("c-native", b"native payload".as_slice()),
        ("c-x86-64", b"x86-64 payload".as_slice()),
        ("c-x86-64-v2", b"x86-64-v2 payload".as_slice()),
        ("c-x86-64-v3", b"x86-64-v3 payload".as_slice()),
        ("c-x86-64-v4", b"x86-64-v4 payload".as_slice()),
    ];

    let mut paths = Vec::new();

    for (name, bytes) in payloads {
        let path = root.join(name);
        fs::write(&path, bytes)?;
        paths.push(path.display().to_string());
    }

    pack_files(&launcher, input.join("valid-packed.spec-elf"), &paths)?;

    fs::write(input.join("empty"), b"")?;
    fs::write(input.join("tiny"), b"abc")?;
    fs::write(input.join("magic-only"), b"VPKFOOT\0")?;

    Ok(())
}