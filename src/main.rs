use crate::archive::format::{is_archive, pack_files, read_back};
use crate::builder::compile::compile_lang;
use std::process::Command;
use std::{self, env, fs, path::Path};

mod arch;
mod archive;
mod builder;

fn main() -> Result<(), anyhow::Error> {
    let current_path = env::current_exe()?;
    let current_name = current_path.file_name().expect("current executable has no file name");

    if is_archive(&current_path)? {
        let correct_exe = read_back(&current_path)?;
        let final_file_path = env::current_dir()?.join(current_name);

        fs::write(&final_file_path, correct_exe)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&final_file_path, fs::Permissions::from_mode(0o755))?;
        }

        return Ok(());
    }

    println!("I hope your running this on project root");

    let dir = env::current_dir()?;
    let dst = compile_lang(dir.to_str().expect("current directory is not valid UTF-8"))?;

    let output_path = dir.join(current_name);
    let pack_output_path = if same_path(&current_path, &output_path) {
        output_path.with_extension("packed")
    } else {
        output_path.clone()
    };

    pack_files(&current_path, &pack_output_path, &dst)?;

    if pack_output_path != output_path {
        fs::rename(&pack_output_path, &output_path)?;
    }

    if !same_path(&current_path, &output_path) {
        let _child = Command::new("rm").arg("-f").arg(current_path).spawn().expect("failed to remove current");
    }

    Ok(())
    //exit program, child starts and deletes us
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}
