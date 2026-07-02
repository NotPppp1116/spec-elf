use spec_elf::archive::format::{is_archive,read_back};
use std::env;
use std::path::PathBuf;

fn main(){
    let Some(path)= env::args().nth(1) else{
        eprint!("errrrrrrr");
        std::process::exit(2);
    };
    let path = PathBuf::from(path);
    let _ = is_archive(&path);
    let _ = read_back(&path);
}