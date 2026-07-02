use spec_elf::archive::format::{is_archive, read_back};
use std::env;
use std::path::PathBuf;

fn main() {
    let Some(path) = env::args_os().nth(1) else {
        std::process::exit(2);
    };

    let path = PathBuf::from(path);

    match is_archive(&path) {
        Ok(true) => {
            let _ = read_back(&path);
        }
        Ok(false) | Err(_) => {}
    }
}