use crate::archive::format::{is_archive, pack_files, read_back};
use crate::builder::compile::{Levels, compile_lang};
use crate::toml::config::check_config;

use anyhow::{Context, Result, bail};
use std::{
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    process::Command,
};

mod arch;
mod archive;
mod builder;
mod toml;

struct Cli {
    target_dir: Option<PathBuf>,
    levels: Option<Levels>,
}

enum CliAction {
    Help,
    Build(Cli),
}
enum Mode {
    Normal,
    Configed,
}
const fn usage() -> &'static str {
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
    let mut mode = Mode::Normal;

    let current_path = env::current_exe()?;
    let current_name = current_path
        .file_name()
        .expect("current executable has no file name");

    if is_archive(&current_path)? {
        return specialize_self(&current_path);
    }

    let args: Vec<String> = env::args().collect();

    if args.is_empty() && check_config()? {
        mode = Mode::Configed;
        
    }

    let action = parse_args(&args).unwrap_or_else(|e| usage_error(&e.to_string()));

    if let CliAction::Help = action {
        println!("{}", usage());
        return Ok(());
    }

    let CliAction::Build(cli) = action else {
        unreachable!();
    };

    if current_name.is_empty() {
        bail!("current executable has an empty file name");
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

fn specialize_self(current_path: &Path) -> Result<()> {
    let app_args: Vec<OsString> = env::args_os().skip(1).collect();
    let correct_exe = read_back(current_path)?;

    replace_current_executable(current_path, &correct_exe)
        .with_context(|| format!("failed to replace {}", current_path.display()))?;

    launch_replacement(current_path, &app_args)
}

fn replace_current_executable(current_path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = current_path
        .parent()
        .context("current executable has no parent directory")?;
    let name = current_path
        .file_name()
        .context("current executable has no file name")?
        .to_string_lossy();
    let temp_path = write_replacement_temp(parent, &name, bytes)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if let Err(error) = fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o755)) {
            let _ = fs::remove_file(&temp_path);
            return Err(error).with_context(|| format!("failed to chmod {}", temp_path.display()));
        }
    }

    if let Err(err) = fs::rename(&temp_path, current_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(err).with_context(|| {
            format!(
                "failed to rename {} over {}",
                temp_path.display(),
                current_path.display()
            )
        });
    }

    Ok(())
}

fn write_replacement_temp(parent: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    for attempt in 0..100 {
        let temp_path = parent.join(format!(
            ".spec-elf-replace-{}-{attempt}-{name}",
            std::process::id()
        ));

        let mut file = match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to create {}", temp_path.display()));
            }
        };

        if let Err(error) = file.write_all(bytes) {
            let _ = fs::remove_file(&temp_path);
            return Err(error).with_context(|| format!("failed to write {}", temp_path.display()));
        }

        return Ok(temp_path);
    }

    bail!(
        "could not create replacement temp file in {}",
        parent.display()
    )
}

#[cfg(unix)]
fn launch_replacement(path: &Path, args: &[OsString]) -> Result<()> {
    use std::os::unix::process::CommandExt;

    Err(Command::new(path).args(args).exec())
        .with_context(|| format!("failed to launch {}", path.display()))
}

#[cfg(not(unix))]
fn launch_replacement(path: &Path, args: &[OsString]) -> Result<()> {
    let status = Command::new(path)
        .args(args)
        .status()
        .with_context(|| format!("failed to launch {}", path.display()))?;

    match status.code() {
        Some(code) => std::process::exit(code),
        None => Ok(()),
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn parse_args_accepts_dir_and_target_levels() {
        let action = parse_args(&args(&["spec-elf", "--dir", "demo", "-ct", "base", "v3"]))
            .expect("valid args");

        let CliAction::Build(cli) = action else {
            panic!("expected build action");
        };

        assert_eq!(cli.target_dir, Some(PathBuf::from("demo")));

        let levels = cli.levels.expect("levels should be selected");
        assert!(levels.contains(Levels::V1));
        assert!(levels.contains(Levels::V3));
        assert!(!levels.contains(Levels::V2));
    }

    #[test]
    fn parse_args_rejects_empty_target_levels() {
        let error = match parse_args(&args(&["spec-elf", "-ct"])) {
            Ok(_) => panic!("expected invalid args"),
            Err(error) => error,
        };

        assert_eq!(error.to_string(), "-ct requires at least one target level");
    }

    #[test]
    fn replace_current_executable_replaces_path_contents() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let exe = dir.path().join("packed-app");

        fs::write(&exe, b"archive bytes")?;

        replace_current_executable(&exe, b"selected payload")?;

        assert_eq!(fs::read(&exe)?, b"selected payload");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = fs::metadata(&exe)?.permissions().mode();
            assert_ne!(mode & 0o111, 0);
        }

        Ok(())
    }
}
