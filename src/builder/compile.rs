use anyhow::{Context, Result, bail};
use std::{
    collections::HashMap,
    fs::{self, read_dir},
    path::{Path, PathBuf},
    process::Command,
};

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Levels: u8 {
        const V1     = 1 << 0;
        const V2     = 1 << 1;
        const V3     = 1 << 2;
        const V4     = 1 << 3;
        const NATIVE = 1 << 4;
    }
}

const C_LEVELS: [(Levels, &str); 5] = [
    (Levels::NATIVE, "-march=native"),
    (Levels::V1, "-march=x86-64"),
    (Levels::V2, "-march=x86-64-v2"),
    (Levels::V3, "-march=x86-64-v3"),
    (Levels::V4, "-march=x86-64-v4"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Idiomes {
    C,
    Cpp,
    Rust,
    Zig,
}

pub fn compile_lang(path: &str, flags: Option<&Levels>) -> Result<Vec<String>> {
    let idiome = find_idiome(path)?;

    match idiome {
        Idiomes::C => compile_c(path, flags),
        Idiomes::Cpp => compile_cpp(path, flags),
        Idiomes::Rust => compile_rust(path, flags),
        Idiomes::Zig => compile_zig(path, flags),
    }
}

fn find_idiome(path: &str) -> Result<Idiomes> {
    let project_dir = project_dir_from_path(path)?;
    let mut counts: HashMap<Idiomes, usize> = HashMap::new();

    count_languages_recursive(&project_dir, &mut counts)?;

    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(idiome, _)| idiome)
        .context("could not detect project language")
}

fn count_languages_recursive(dir: &Path, counts: &mut HashMap<Idiomes, usize>) -> Result<()> {
    for entry in read_dir(dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        let path = entry.path();

        if path.is_dir() {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            if matches!(name, "target" | "build" | ".git") {
                continue;
            }

            count_languages_recursive(&path, counts)?;
            continue;
        }

        let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };

        let idiome = match ext {
            "c" | "h" => Idiomes::C,
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Idiomes::Cpp,
            "rs" => Idiomes::Rust,
            "zig" => Idiomes::Zig,
            _ => continue,
        };

        *counts.entry(idiome).or_insert(0) += 1;
    }

    Ok(())
}

fn compile_c(path: &str, flags: Option<&Levels>) -> Result<Vec<String>> {
    let project_dir = project_dir_from_path(path)?;
    let build_dir = project_dir.join("build");
    fs::create_dir_all(&build_dir)?;

    let sources = collect_sources(&project_dir, &["c"])?;

    if sources.is_empty() {
        bail!("no C source files found");
    }

    let has_cmake = project_dir.join("CMakeLists.txt").exists();

    let marches = filter_c_flags(flags);
    if marches.is_empty() {
        bail!("no C target levels selected");
    }

    let mut outputs = Vec::with_capacity(marches.len());

    for march in marches {
        let march_name = march.trim_start_matches("-march=");
        let output = build_dir.join(format!("c-{march_name}"));

        if has_cmake {
            let cmake_build_dir = build_dir.join(format!("cmake-{march_name}"));
            let cmake_output_dir = build_dir.join(format!("cmake-c-out-{march_name}"));

            fs::create_dir_all(&cmake_output_dir)?;

            let status = Command::new("cmake")
                .arg("-S")
                .arg(&project_dir)
                .arg("-B")
                .arg(&cmake_build_dir)
                .arg("-DCMAKE_BUILD_TYPE=Release")
                .arg(format!("-DCMAKE_C_FLAGS_RELEASE=-O3 {march}"))
                .arg(format!(
                    "-DCMAKE_RUNTIME_OUTPUT_DIRECTORY={}",
                    cmake_output_dir.display()
                ))
                .arg(format!(
                    "-DCMAKE_RUNTIME_OUTPUT_DIRECTORY_RELEASE={}",
                    cmake_output_dir.display()
                ))
                .current_dir(&project_dir)
                .status()
                .with_context(|| format!("could not configure cmake for {march_name}"))?;

            if !status.success() {
                bail!("cmake configure failed for {march_name} with status {status}");
            }

            let status = Command::new("cmake")
                .arg("--build")
                .arg(&cmake_build_dir)
                .arg("--config")
                .arg("Release")
                .current_dir(&project_dir)
                .status()
                .with_context(|| format!("could not build cmake project for {march_name}"))?;

            if !status.success() {
                bail!("cmake build failed for {march_name} with status {status}");
            }

            let built_exe = find_single_executable(&cmake_output_dir)
                .with_context(|| format!("could not find cmake executable for {march_name}"))?;

            if output.exists() {
                fs::remove_file(&output)?;
            }

            fs::copy(&built_exe, &output)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&output, fs::Permissions::from_mode(0o755))?;
            }
        } else {
            let mut command = Command::new("gcc");

            command.arg("-O3").arg(march).arg("-Iinclude").arg("-Isrc");

            for source in &sources {
                command.arg(source);
            }

            let status = command
                .arg("-o")
                .arg(&output)
                .current_dir(&project_dir)
                .status()
                .with_context(|| format!("could not run gcc for {march_name}"))?;

            if !status.success() {
                bail!("gcc failed for {march_name} with status {status}");
            }
        }

        outputs.push(output.display().to_string());
    }

    Ok(outputs)
}
fn compile_cpp(path: &str, flags: Option<&Levels>) -> Result<Vec<String>> {
    let project_dir = project_dir_from_path(path)?;
    let build_dir = project_dir.join("build");
    fs::create_dir_all(&build_dir)?;

    let sources = collect_sources(&project_dir, &["cpp", "cc", "cxx"])?;

    if sources.is_empty() {
        bail!("no C++ source files found");
    }

    let has_cmake = project_dir.join("CMakeLists.txt").is_file();

    let marches = filter_c_flags(flags);
    if marches.is_empty() {
        bail!("no cpp targets selected");
    }

    let mut outputs = Vec::with_capacity(marches.len());

    for march in marches {
        let march_name = march.trim_start_matches("-march=");
        let output = build_dir.join(format!("cpp-{march_name}"));

        if has_cmake {
            let cmake_build_dir = build_dir.join(format!("cmake-cpp-{march_name}"));
            let cmake_output_dir = build_dir.join(format!("cmake-cpp-out-{march_name}"));

            fs::create_dir_all(&cmake_output_dir)?;

            let status = Command::new("cmake")
                .arg("-S")
                .arg(&project_dir)
                .arg("-B")
                .arg(&cmake_build_dir)
                .arg("-DCMAKE_BUILD_TYPE=Release")
                .arg(format!("-DCMAKE_CXX_FLAGS_RELEASE=-O3 {march}"))
                .arg(format!(
                    "-DCMAKE_RUNTIME_OUTPUT_DIRECTORY={}",
                    cmake_output_dir.display()
                ))
                .arg(format!(
                    "-DCMAKE_RUNTIME_OUTPUT_DIRECTORY_RELEASE={}",
                    cmake_output_dir.display()
                ))
                .current_dir(&project_dir)
                .status()
                .with_context(|| format!("could not configure cmake for {march_name}"))?;

            if !status.success() {
                bail!("cmake configure failed for {march_name} with status {status}");
            }

            let status = Command::new("cmake")
                .arg("--build")
                .arg(&cmake_build_dir)
                .arg("--config")
                .arg("Release")
                .current_dir(&project_dir)
                .status()
                .with_context(|| format!("could not build cmake project for {march_name}"))?;

            if !status.success() {
                bail!("cmake build failed for {march_name} with status {status}");
            }

            let built_exe = find_single_executable(&cmake_output_dir)
                .with_context(|| format!("could not find cmake executable for {march_name}"))?;

            if output.exists() {
                fs::remove_file(&output)?;
            }

            fs::copy(&built_exe, &output)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                fs::set_permissions(&output, fs::Permissions::from_mode(0o755))?;
            }
        } else {
            let mut command = Command::new("g++");

            command
                .arg("-O3")
                .arg(march)
                .arg("-I.")
                .arg("-Iinclude")
                .arg("-Isrc");

            for source in &sources {
                command.arg(source);
            }

            let status = command
                .arg("-o")
                .arg(&output)
                .current_dir(&project_dir)
                .status()
                .with_context(|| format!("could not run g++ for {march_name}"))?;

            if !status.success() {
                bail!("g++ failed for {march_name} with status {status}");
            }
        }

        outputs.push(output.display().to_string());
    }

    Ok(outputs)
}

fn compile_zig(path: &str, flags: Option<&Levels>) -> Result<Vec<String>> {
    let project_dir = project_dir_from_path(path)?;
    let build_dir = project_dir.join("build");
    fs::create_dir_all(&build_dir)?;

    let source = if Path::new(path).is_file() {
        PathBuf::from(path)
    } else {
        find_first_source(&project_dir, &["zig"])?
    };

    const ZIG_LEVELS: [(Levels, &str); 5] = [
        (Levels::NATIVE, "-mcpu=native"),
        (Levels::V1, "-mcpu=x86_64"),
        (Levels::V2, "-mcpu=x86_64_v2"),
        (Levels::V3, "-mcpu=x86_64_v3"),
        (Levels::V4, "-mcpu=x86_64_v4"),
    ];

    let marches: Vec<&str> = match flags {
        Some(flags) => ZIG_LEVELS
            .iter()
            .filter_map(|(level, march)| {
                if flags.contains(*level) {
                    Some(*march)
                } else {
                    None
                }
            })
            .collect(),

        None => ZIG_LEVELS.iter().map(|(_, march)| *march).collect(),
    };

    if marches.is_empty() {
        bail!("no Zig target levels selected");
    }

    let mut outputs = Vec::with_capacity(marches.len());

    for march in marches {
        let name = march.trim_start_matches("-mcpu=");
        let output = build_dir.join(format!("zig-{name}"));
        let emit = format!("-femit-bin={}", output.display());

        let status = Command::new("zig")
            .arg("build-exe")
            .arg(&source)
            .arg("-O")
            .arg("ReleaseFast")
            .arg(march)
            .arg(&emit)
            .current_dir(&project_dir)
            .status()
            .with_context(|| format!("could not run zig for {name}"))?;

        if !status.success() {
            bail!("zig failed for {name} with status {status}");
        }

        outputs.push(output.display().to_string());
    }

    Ok(outputs)
}

const RUST_LEVELS: [(Levels, &str, &str); 5] = [
    (Levels::NATIVE, "native", "-C target-cpu=native"),
    (Levels::V1, "x86_64", "-C target-cpu=x86-64"),
    (Levels::V2, "x86_64_v2", "-C target-cpu=x86-64-v2"),
    (Levels::V3, "x86_64_v3", "-C target-cpu=x86-64-v3"),
    (Levels::V4, "x86_64_v4", "-C target-cpu=x86-64-v4"),
];

pub fn compile_rust(path: &str, flags: Option<&Levels>) -> Result<Vec<String>> {
    let project_dir = find_cargo_project_dir(path)?;
    let build_dir = project_dir.join("build");
    fs::create_dir_all(&build_dir)?;

    let package_name = cargo_package_name(&project_dir)?;

    const TARGET_TRIPLE: &str = "x86_64-unknown-linux-gnu";
    const TARGET_RUSTFLAGS_ENV: &str = "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS";

    let targets: Vec<(&str, &str)> = match flags {
        Some(flags) => RUST_LEVELS
            .iter()
            .filter_map(|(level, name, rustflags)| {
                if flags.contains(*level) {
                    Some((*name, *rustflags))
                } else {
                    None
                }
            })
            .collect(),

        None => RUST_LEVELS
            .iter()
            .map(|(_, name, rustflags)| (*name, *rustflags))
            .collect(),
    };

    if targets.is_empty() {
        bail!("no Rust target levels selected");
    }

    let mut outputs = Vec::with_capacity(targets.len());

    for (name, rustflags) in targets {
        let target_dir = project_dir.join("target").join(format!("rust-{name}"));
        let output = build_dir.join(format!("rust-{name}"));

        let status = Command::new("cargo")
            .args(["build", "--release", "--target", TARGET_TRIPLE])
            .current_dir(&project_dir)
            .env_remove("RUSTFLAGS")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .env_remove("CARGO_BUILD_RUSTFLAGS")
            .env(TARGET_RUSTFLAGS_ENV, rustflags)
            .env("CARGO_TARGET_DIR", &target_dir)
            .status()
            .with_context(|| format!("failed to run cargo for {name}"))?;

        if !status.success() {
            bail!("cargo failed for {name} with status {status}");
        }

        let built_bin = target_dir
            .join(TARGET_TRIPLE)
            .join("release")
            .join(&package_name);

        fs::copy(&built_bin, &output).with_context(|| {
            format!(
                "failed to copy built binary from {} to {}",
                built_bin.display(),
                output.display()
            )
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&output, fs::Permissions::from_mode(0o755))?;
        }

        outputs.push(output.display().to_string());
    }

    Ok(outputs)
}

fn project_dir_from_path(path: &str) -> Result<PathBuf> {
    let path = Path::new(path);

    if path.is_file() {
        return path
            .parent()
            .map(Path::to_path_buf)
            .context("file path has no parent directory");
    }

    Ok(path.to_path_buf())
}

fn find_cargo_project_dir(path: &str) -> Result<PathBuf> {
    let mut dir = project_dir_from_path(path)?;

    loop {
        if dir.join("Cargo.toml").is_file() {
            return Ok(dir);
        }

        if !dir.pop() {
            bail!("could not find Cargo.toml");
        }
    }
}

fn cargo_package_name(project_dir: &Path) -> Result<String> {
    let cargo_toml_path = project_dir.join("Cargo.toml");
    let cargo_toml = fs::read_to_string(&cargo_toml_path)
        .with_context(|| format!("failed to read {}", cargo_toml_path.display()))?;

    for line in cargo_toml.lines() {
        let line = line.trim();

        if let Some(name) = line.strip_prefix("name = ") {
            return Ok(name.trim_matches('"').to_string());
        }
    }

    bail!("could not find package name in Cargo.toml");
}

fn collect_sources(project_dir: &Path, extensions: &[&str]) -> Result<Vec<PathBuf>> {
    let mut sources = Vec::new();
    collect_sources_recursive(project_dir, extensions, &mut sources)?;
    Ok(sources)
}

fn collect_sources_recursive(
    dir: &Path,
    extensions: &[&str],
    sources: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in read_dir(dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        let path = entry.path();

        if path.is_dir() {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            if matches!(name, "target" | "build" | ".git") {
                continue;
            }

            collect_sources_recursive(&path, extensions, sources)?;
            continue;
        }

        let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };

        if extensions.contains(&ext) {
            sources.push(path);
        }
    }

    Ok(())
}

fn find_single_executable(dir: &Path) -> Result<PathBuf> {
    let mut built_exe = None;

    for entry in fs::read_dir(dir)? {
        let path = entry?.path();

        if !path.is_file() {
            continue;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = path.metadata()?.permissions().mode();

            if mode & 0o111 == 0 {
                continue;
            }
        }

        if built_exe.is_some() {
            bail!("cmake produced multiple executables in {}", dir.display());
        }

        built_exe = Some(path);
    }

    built_exe.with_context(|| format!("cmake produced no executable in {}", dir.display()))
}

fn find_first_source(project_dir: &Path, extensions: &[&str]) -> Result<PathBuf> {
    let sources = collect_sources(project_dir, extensions)?;

    sources
        .into_iter()
        .next()
        .context("could not find source file")
}
fn filter_c_flags(flags: Option<&Levels>) -> Vec<&'static str> {
    match flags {
        Some(flags) => C_LEVELS
            .iter()
            .filter_map(|(level, march)| {
                if flags.contains(*level) {
                    Some(*march)
                } else {
                    None
                }
            })
            .collect(),

        None => C_LEVELS.iter().map(|(_, march)| *march).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_c_flags_defaults_to_every_level() {
        assert_eq!(
            filter_c_flags(None),
            vec![
                "-march=native",
                "-march=x86-64",
                "-march=x86-64-v2",
                "-march=x86-64-v3",
                "-march=x86-64-v4",
            ]
        );
    }

    #[test]
    fn filter_c_flags_keeps_only_selected_levels() {
        let levels = Levels::V2 | Levels::V4;

        assert_eq!(
            filter_c_flags(Some(&levels)),
            vec!["-march=x86-64-v2", "-march=x86-64-v4"]
        );
    }
}
