use crate::archive::format::{is_archive, pack_files, read_back};
use crate::builder::compile::compile_lang;
use std::io::ErrorKind;
use std::process::Command;
use std::{self, env, fs, path::Path};

mod arch;
mod archive;
mod builder;

#[derive(PartialEq)]
enum Args {
    No,
    Yes,
}

fn usage() -> &'static str {
    "usage: spec-elf [--dir <target-dir>]\n\nRun with no arguments from the target project directory, or pass --dir followed by the target project directory."
}

fn help() -> ! {
    println!("{}", usage());
    std::process::exit(0);
}

fn usage_error(message: &str) -> ! {
    eprintln!("error: {message}\n\n{}", usage());
    std::process::exit(2);
}

fn is_help_flag(arg: &str) -> bool {
    let arg = arg.to_ascii_lowercase();
    matches!(arg.as_str(), "--help" | "-help" | "-h" | "--h")
}

fn is_dir_flag(arg: &str) -> bool {
    let arg = arg.to_ascii_lowercase();
    matches!(arg.as_str(), "--dir" | "-dir")
}

fn main() -> Result<(), anyhow::Error> {
    let args: Vec<String> = env::args().collect();

    let has_args = if args.len() > 1 { Args::Yes } else { Args::No };

    // Accept only the supported invocation shapes here. Unknown arguments must
    // stop before the build path, otherwise a typo like `spec-elf foo` would
    // accidentally build the current directory.
    let target_dir = match args.as_slice() {
        [_program] => None,
        [_program, flag] if is_help_flag(flag) => help(),
        [_program, flag, dir] if is_dir_flag(flag) && !dir.is_empty() => Some(dir.as_str()),
        [_program, flag] if is_dir_flag(flag) => usage_error("--dir requires a target directory"),
        [_program, flag, _dir] if is_dir_flag(flag) => {
            usage_error("--dir requires exactly one target directory")
        }
        [_program, flag, ..] if is_help_flag(flag) => {
            usage_error("help flags do not take extra arguments")
        }
        _ => usage_error("invalid arguments"),
    };

    let current_path = env::current_exe()?;
    let current_name = current_path
        .file_name()
        .expect("current executable has no file name");

    // A packed spec-elf binary is also a valid launcher. If the current
    // executable already contains a footer, this run is the runtime path:
    // extract the best payload for this machine and launch it.
    if is_archive(&current_path)? {
        let correct_exe = read_back(&current_path)?;
        let final_file_path = env::current_dir()?.join(current_name);

        fs::write(&final_file_path, correct_exe)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&final_file_path, fs::Permissions::from_mode(0o755))?;
        }

        #[allow(clippy::zombie_processes)]
        Command::new(final_file_path).spawn()?;

        return Ok(());
    }

    // In build mode, --dir changes the target project directory before
    // language detection and compilation start.
    if has_args == Args::Yes {
        let Some(target_dir) = target_dir else {
            usage_error("invalid arguments");
        };

        match env::set_current_dir(target_dir) {
            Ok(_) => {}
            Err(e) => {
                match e.kind() {
                    ErrorKind::NotFound => println!("directory not found"),
                    ErrorKind::PermissionDenied => println!("wrong permissions"),
                    ErrorKind::NotADirectory => println!("this is not a dir"),
                    _ => println!("idk this error"),
                }

                return Err(e.into());
            }
        }
    }

    let dir = env::current_dir()?;
    let dst = compile_lang(dir.to_str().expect("current directory is not valid UTF-8"))?;

    let output_path = dir.join(current_name);

    // When spec-elf is run from the same directory where it will write the
    // packed output, avoid truncating the running executable while it is still
    // being copied by writing to a temporary sibling first.
    let pack_output_path = if same_path(&current_path, &output_path) {
        output_path.with_extension("packed")
    } else {
        output_path.clone()
    };

    pack_files(&current_path, &pack_output_path, &dst)?;

    if pack_output_path != output_path {
        let parent = output_path.parent().expect("couldnt catch parent");
    
        let new_name = output_path
            .file_name()
            .expect("filename")
            .to_str()
            .unwrap();
    
        let mut new_name = new_name.to_string();
        new_name.push_str(".spec-elf");
    
        let np = parent.join(new_name);
    
        fs::rename(&pack_output_path, &np)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(&output_path, fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}

/// Compare two paths after canonicalization when possible.
///
/// This keeps the self-overwrite check working even when paths are written in
/// different forms, for example `./spec-elf` and `/home/user/project/spec-elf`.
fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}
