use crate::archive::format::{is_archive, pack_files, read_back};
use crate::builder::compile::{compile_lang, Levels};

use anyhow::{bail, Context, Result};
use std::{
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
};

mod arch;
mod archive;
mod builder;

struct Cli {
    target_dir: Option<PathBuf>,
    levels: Option<Levels>,
}

enum CliAction {
    Help,
    Build(Cli),
}

fn usage() -> &'static str {
    "usage: spec-elf [--dir <target-dir>] [-ct <levels...>]

Run with no arguments from the target project directory, or pass --dir followed by the target project directory.

Target levels:
  base     x86-64 baseline
  v1       x86-64 baseline
  v2       x86-64-v2
  v3       x86-64-v3
  v4       x86-64-v4
  native   current CPU

Examples:
  spec-elf
  spec-elf --dir ./my-project
  spec-elf -ct base v2 v3
  spec-elf --dir ./my-project -ct native"
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

fn is_ct_flag(arg: &str) -> bool {
    let arg = arg.to_ascii_lowercase();
    matches!(arg.as_str(), "-ct" | "--ct")
}

fn parse_level(arg: &str) -> Result<Levels> {
    match arg.to_ascii_lowercase().as_str() {
        "base" | "v1" => Ok(Levels::V1),
        "v2" => Ok(Levels::V2),
        "v3" => Ok(Levels::V3),
        "v4" => Ok(Levels::V4),
        "native" => Ok(Levels::NATIVE),
        _ => bail!("unknown target level: {arg}"),
    }
}

fn parse_args(args: &[String]) -> Result<CliAction> {
    if args.len() == 2 && is_help_flag(&args[1]) {
        return Ok(CliAction::Help);
    }

    let mut target_dir = None;
    let mut levels = None;

    let mut i = 1;

    while i < args.len() {
        let arg = args[i].as_str();

        if is_help_flag(arg) {
            bail!("help flags do not take extra arguments");
        }

        if is_dir_flag(arg) {
            i += 1;

            let Some(dir) = args.get(i) else {
                bail!("--dir requires a target directory");
            };

            if dir.starts_with('-') {
                bail!("--dir requires a target directory");
            }

            if target_dir.is_some() {
                bail!("--dir was passed more than once");
            }

            target_dir = Some(PathBuf::from(dir));
            i += 1;
            continue;
        }

        if is_ct_flag(arg) {
            i += 1;

            let mut selected = levels.unwrap_or_else(Levels::empty);
            let mut saw_level = false;

            while let Some(level_arg) = args.get(i) {
                if level_arg.starts_with('-') {
                    break;
                }

                selected |= parse_level(level_arg)?;
                saw_level = true;
                i += 1;
            }

            if !saw_level {
                bail!("-ct requires at least one target level");
            }

            levels = Some(selected);
            continue;
        }

        bail!("unknown argument: {arg}");
    }

    Ok(CliAction::Build(Cli { target_dir, levels }))
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    let action = parse_args(&args).unwrap_or_else(|e| usage_error(&e.to_string()));

    if let CliAction::Help = action {
        println!("{}", usage());
        return Ok(());
    }

    let CliAction::Build(cli) = action else {
        unreachable!();
    };

    let current_path = env::current_exe()?;
    let current_name = current_path
        .file_name()
        .expect("current executable has no file name");

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

    if let Some(target_dir) = &cli.target_dir {
        match env::set_current_dir(target_dir) {
            Ok(()) => {}
            Err(e) => {
                match e.kind() {
                    ErrorKind::NotFound => eprintln!("directory not found"),
                    ErrorKind::PermissionDenied => eprintln!("wrong permissions"),
                    ErrorKind::NotADirectory => eprintln!("this is not a dir"),
                    _ => eprintln!("could not change directory"),
                }

                return Err(e.into());
            }
        }
    }

    let dir = env::current_dir()?;

    let dir_str = dir
        .to_str()
        .context("current directory is not valid UTF-8")?;

    let dst = compile_lang(dir_str, cli.levels.as_ref())?;

    let output_base_path = dir.join(current_name);

    let output_path = output_base_path.with_file_name(format!(
        "{}.spec-elf",
        output_base_path
            .file_name()
            .expect("filename")
            .to_string_lossy()
    ));

    let pack_output_path = if same_path(&current_path, &output_path) {
        output_path.with_file_name(format!(
            "{}.tmp",
            output_path.file_name().expect("filename").to_string_lossy()
        ))
    } else {
        output_path.clone()
    };

    pack_files(&current_path, &pack_output_path, &dst)?;

    if pack_output_path != output_path {
        fs::rename(&pack_output_path, &output_path)?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&output_path, fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}