# spec-elf

`spec-elf` is an experimental launcher/packer for native executable projects.

It builds several versions of a project for different x86-64 CPU targets, packs those binaries behind a launcher, and later runs the payload that best matches the current machine.

The goal is simple: one packed executable that can carry multiple optimized builds.

> Experimental: the file format and runtime behavior are still changing. Do not use this for untrusted binaries or important production software yet.

## What it does

When you run `spec-elf` on a target project, it:

1. detects the project language
2. builds several optimized binaries for different CPU targets
3. writes those binaries into the target project's `build/` directory
4. appends the binaries to the `spec-elf` launcher
5. writes a manifest and footer at the end of the launcher
6. produces a packed executable in the target project directory

When you run the packed executable, it:

1. opens its own executable file
2. reads the footer and manifest
3. detects the current CPU level
4. extracts the best matching payload
5. starts that payload

## Supported project types

`spec-elf` currently detects the dominant language in the target directory by counting source-file extensions.

| Language | Detection | Builder |
| --- | --- | --- |
| C | `.c`, `.h` | `gcc`, or CMake if `CMakeLists.txt` exists |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hxx` | `g++`, or CMake if `CMakeLists.txt` exists |
| Rust | `.rs` | `cargo build --release` with per-target `RUSTFLAGS` |
| Zig | `.zig` | `zig build-exe -O ReleaseFast` |

Generated payloads are written under the target project's `build/` directory.

## CPU targets

For C and C++, `spec-elf` currently builds:

```text
-march=native
-march=x86-64
-march=x86-64-v2
-march=x86-64-v3
-march=x86-64-v4
```

For Rust, it uses equivalent `target-cpu` values:

```text
native
x86-64
x86-64-v2
x86-64-v3
x86-64-v4
```

For Zig, it uses equivalent `-mcpu` values:

```text
native
x86_64
x86_64_v2
x86_64_v3
x86_64_v4
```

## Requirements

You need Rust installed to build `spec-elf` itself.

Depending on the target project language, you may also need:

- `gcc` for C projects
- `g++` for C++ projects
- `cmake` for C/C++ projects that use CMake
- `cargo` for Rust projects
- `zig` for Zig projects

## Build spec-elf

From this repository:

```bash
cargo build --release
```

The launcher binary will be at:

```text
target/release/spec-elf
```

## Usage

Run it from the project you want to pack:

```bash
/path/to/spec-elf
```

Or pass the target directory explicitly:

```bash
/path/to/spec-elf --dir /path/to/project
```

The packed output is written into the target project directory using the same file name as the launcher.

For example, if the launcher is named `spec-elf`, the output in the target directory will also be named `spec-elf`.

## What the packed file contains

The packed executable is laid out like this:

```text
[launcher executable bytes]
[payload 0 bytes]
[payload 1 bytes]
[...]
[manifest]
[footer]
```

The launcher remains at the start of the file, so the operating system can still execute it normally.

The footer is fixed-size and lives at the end of the file. It points back to the manifest, which describes the packed payloads.

For the exact format, see [`docs/format.md`](docs/format.md).

## Runtime payload selection

When a packed executable starts, it prefers the `native` payload only when the stored native CPU hash matches the current machine.

If the native hash does not match, it falls back to x86-64 level selection:

```text
x86-64
x86-64-v2
x86-64-v3
x86-64-v4
```

The launcher expects the packed payload set to contain a compatible target.

## Safety notes

`spec-elf` is not a sandbox.

Do not run packed executables from people you do not trust. A packed file contains native executable payloads, and the launcher will write and start one of them.

## Current limitations

- x86-64-focused target selection only
- experimental packed file format
- no stable compatibility promise yet
- simple language detection based on file extensions
- no config file yet
- manual C/C++ builds are intentionally basic
- CMake projects that produce multiple executables may need explicit selection in the future
- not a security sandbox

## Developer checks

Useful commands while working on the repository:

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -W clippy::pedantic
cargo test
cargo build --release
```

The archive reader/writer code is a good fuzzing target because it parses offsets, sizes, names, and footer fields from a file.

## More documentation

- [`docs/format.md`](docs/format.md): packed executable layout
- [`docs/builders.md`](docs/builders.md): builder behavior for C, C++, Rust, and Zig
