use crate::compile_logic::language::compile_lang;
use crate::package_logic::format::pack_files;
use std::process::Command;
use std::{self, env, io};

mod compile_logic;
mod package_logic;

fn main() -> Result<(), anyhow::Error> {
    let args: Vec<String> = env::args().collect();


    println!("I hope your running this on project root");

    let dir = env::current_dir();
    let dir = dir.unwrap();

    let dst = compile_lang(dir.to_str().unwrap())?;

    let launcher: String = env::current_exe().unwrap().to_string_lossy().into_owned();

    let _ = pack_files(&launcher, "out", &dst);

    let name = env::current_exe().unwrap();

    let _child = Command::new("rm").arg("-f").arg(name).spawn().expect("failed to remove current");

    Ok(())
    //exit program, child starts and deletes us
}
