use std::{
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use anyhow::Result;

use crate::archive::format::{FOOTER_MAGIC, FOOTER_SIZE};

fn inspect(path: &Path) -> Result<()> {
    let mut file = OpenOptions::new().read(true).open(path)?;

    let file_size = file.metadata()?.len();

    if file_size < FOOTER_SIZE {
        println!("file is too small to be an archive");
        return Ok(());
    }

    let mut footer_magic = [0u8; 8];

    file.seek(SeekFrom::End(-(FOOTER_SIZE as i64)))?;
    file.read_exact(&mut footer_magic)?;

    if footer_magic != *FOOTER_MAGIC {
        println!("file isnt an archive");
        return Ok(());
    }

    let mut manifest_offset = [0u8; size_of::<u64>()];
    file.read_exact(&mut manifest_offset)?;

    let mut manifest_size = [0u8; size_of::<u64>()];
    file.read_exact(&mut manifest_size)?;

    let mut native_hash = [0u8; size_of::<u64>()];
    file.read_exact(&mut native_hash)?;

    let mut was_launched = [0u8; 1];
    file.read_exact(&mut was_launched)?;

    assert_eq!(
        footer_magic.len()
            + manifest_offset.len()
            + manifest_size.len()
            + native_hash.len()
            + was_launched.len(),
        FOOTER_SIZE as usize
    );

    let name = path.file_name().unwrap().to_string_lossy();

    let value = match was_launched[0] {
        0 => "false",
        1 => "true",
        _ => "invalid",
    };

    let manifest_offset = u64::from_le_bytes(manifest_offset);
    let manifest_size = u64::from_le_bytes(manifest_size);
    let native_hash = u64::from_le_bytes(native_hash);

    let output = format!(
        "{name}:\n  footer_magic: {:?}\n  manifest offset: {manifest_offset}\n  manifest size: {manifest_size}\n  native hash: {native_hash}\n  was launched: {value}\n",
        footer_magic,
    );

    print!("{output}");

    Ok(())
}