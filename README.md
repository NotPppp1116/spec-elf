# spec-elf

**One executable. Multiple CPU-specialized builds. First-launch specialization.**

`spec-elf` is a native executable launcher and packer that builds a project several times for different x86-64 CPU targets, compresses those builds, and packs them into one runnable file.

The packed file is not meant to be a permanent runtime dispatch layer. Its job is to specialize itself for the machine it is launched on: choose the best matching payload, materialize that optimized executable, and start it.

The end goal is simple: distribute one file, but run code that was built for the CPU in front of it.

## The problem

When you distribute a native executable, you normally have to build for the oldest CPU level you want to support.

For x86-64, that often means targeting a very conservative baseline. That baseline exists so the binary can run almost everywhere, including very old machines with weaker cores and fewer instruction-set features.

That portability has a cost. Newer CPUs can support better instructions, wider vector features, and stronger optimization targets, but a single generic binary usually cannot fully use them.

The usual alternatives are not great:

- ship one baseline binary and leave performance on the table
- ship a `native` binary and limit who can run it
- publish several binaries and make users choose the correct one
- use shell scripts or install-time logic to dispatch between builds

`spec-elf` tackles that problem directly: keep the distribution model simple, but let the executable carry multiple optimized builds inside itself.

## What makes it interesting

Most native projects ship a baseline executable because it is portable. That leaves performance on the table for newer CPUs.

`spec-elf` takes a different approach:

- build a baseline binary
- build newer x86-64 level variants
- build a native variant for the packing machine
- compress all payloads
- append them behind a normal launcher executable
- use the launcher as a specialization step
- materialize the best compatible payload as a normal optimized executable

This makes CPU-specific builds much easier to distribute and test without needing users to manually pick from several downloads.

## Current status

`spec-elf` is already usable for experimenting with real native projects.

The core pipeline works:

1. detect the project language
2. build several optimized payloads
3. pack them into one executable
4. store a manifest and footer
5. detect the current CPU on launch
6. extract and start the best matching payload

The format is still evolving, but the current project is not just a sketch. It already has a working build path, specialization path, CPU selection, compression, manifest/footer layout, and tests around the archive behavior.

## Supported languages

`spec-elf` currently supports projects written in:

| Language | Build path |
| --- | --- |
| C | `gcc`, or CMake when `CMakeLists.txt` exists |
| C++ | `g++`, or CMake when `CMakeLists.txt` exists |
| Rust | `cargo build --release` with per-target CPU flags |
| Zig | `zig build-exe -O ReleaseFast` |

Language detection is automatic. `spec-elf` scans the target project and picks the dominant source language based on file extensions.

Ignored directories include:

```text
target
build
.git
```

## CPU targets

For C and C++, `spec-elf` builds these variants:

```text
-march=native
-march=x86-64
-march=x86-64-v2
-march=x86-64-v3
-march=x86-64-v4
```

For Rust, it uses matching `target-cpu` values:

```text
native
x86-64
x86-64-v2
x86-64-v3
x86-64-v4
```

For Zig, it uses matching `-mcpu` values:

```text
native
x86_64
x86_64_v2
x86_64_v3
x86_64_v4
```

## How packing works

A packed `spec-elf` executable keeps the launcher at the start of the file. That means the operating system can still execute it normally.

After the launcher bytes, `spec-elf` appends compressed payloads, a manifest, and a fixed-size footer:

```text
[launcher executable]
[compressed payload 0]
[compressed payload 1]
[compressed payload 2]
[...]
[manifest]
[footer]
```

The manifest stores the name, offset, and size of each payload.

The footer lives at the very end of the file and points back to the manifest. This lets the launcher quickly detect whether it is running as a normal builder binary or as a packed executable.

## First-launch specialization

When a packed executable starts, `spec-elf` opens its own file and reads the footer and manifest.

It then selects a payload like this:

1. If the packed file contains a `native` payload and the stored native CPU hash matches the current machine, use the native payload.
2. Otherwise, detect the current x86-64 level.
3. Select the best available portable payload at or below the detected level.

Supported levels:

```text
x86-64
x86-64-v2
x86-64-v3
x86-64-v4
```

After choosing the payload, the launcher decompresses that payload, writes it to a temporary sibling path, marks it executable on Unix systems, and atomically renames it over the packed archive. It then starts the replacement executable with the original command-line arguments.

That replacement executable is just the selected optimized program. It does not need the packed archive, the other CPU variants, or the `spec-elf` selection path. Subsequent launches of the same path run the optimized program directly.

The native hash includes CPU and platform information, so the `native` payload is only reused when it matches the machine it was built for. If it does not match, the launcher falls back to portable x86-64 level selection.

Portable fallback never chooses a higher CPU level than the current machine supports. For example, an x86-64-v4 machine can use v4, v3, v2, or baseline payloads, in that order; an x86-64-v3 machine will not use a v4 payload.

## Build spec-elf

From this repository:

```bash
cargo build --release
```

The launcher will be built at:

```text
target/release/spec-elf
```

## Usage

Pack the current project:

```bash
/path/to/spec-elf
```

Or pack a specific project directory:

```bash
/path/to/spec-elf --dir /path/to/project
```

The packed executable is written into the target project directory with a `.spec-elf` suffix.

Build only selected CPU targets with `-ct`:

```bash
/path/to/spec-elf -ct base v2 v3
/path/to/spec-elf --dir /path/to/project -ct native
```

For example, if the launcher is named:

```text
spec-elf
```

the packed output will be:

```text
spec-elf.spec-elf
```

## Example flow

```bash
cargo build --release

cd /path/to/native/project
/path/to/spec-elf

./spec-elf.spec-elf
```

On launch, the packed file chooses the best payload for the current CPU, materializes the selected optimized executable, and starts it.

## Requirements

To build `spec-elf` itself:

- Rust

Depending on the project being packed:

- `gcc` for C projects
- `g++` for C++ projects
- `cmake` for C/C++ projects that use CMake
- `cargo` for Rust projects
- `zig` for Zig projects

## Project layout

```text
src/
  main.rs              CLI, build/runtime mode selection
  arch/
    x86.rs             x86-64 level detection and native CPU hashing
  archive/
    format.rs          packed file writer/reader
  builder/
    compile.rs         C, C++, Rust, and Zig build logic

docs/
  format.md            packed executable format
  builders.md          builder behavior
```

## Format overview

The current footer contains:

```text
u8[8] magic
u64   manifest_offset
u64   manifest_size
u64   native_hash
u8    launch_flag
```

The current magic value is:

```text
VPKFOOT\0
```

The manifest contains one entry per payload:

```text
u32 entry_count

repeated entry_count times:
    u32 name_len
    u8[name_len] name_utf8
    u64 payload_offset
    u64 payload_size
```

See [`docs/format.md`](docs/format.md) for more detail.

## Current strengths

- single-file distribution for multiple optimized native builds
- first-launch CPU specialization instead of permanent user-facing dispatch
- native payload fast path for the build machine
- compressed payload storage with zstd
- support for C, C++, Rust, and Zig projects
- CMake support for C/C++ projects
- simple append-only packed format
- normal executable header remains intact
- clear separation between builder mode and packed-launcher mode

## Current limitations

`spec-elf` is focused and intentionally small right now.

Current limitations:

- x86-64 only
- Linux-focused Rust target path for Rust builds
- no config file yet
- language detection is based on source extension counts
- CMake projects that produce multiple executables need explicit target selection in the future
- packed format is still allowed to evolve

These are design and polish limitations, not blockers for the main idea. The core concept is already implemented and useful.

## Developer checks

Useful commands while working on the repository:

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -W clippy::pedantic
cargo test
cargo build --release
```

GitHub CI runs formatting, clippy, and tests on pushes and pull requests.

The archive reader/writer is especially important to test because it handles offsets, sizes, names, compressed payloads, and footer fields.

## Roadmap

Good next steps:

- add explicit CLI flags for language selection
- add explicit CLI flags for executable target selection
- add a project config file
- support more target families
- improve docs with real project examples
- add fuzzing for archive parsing
- add more tests around payload selection and corrupted archives

## Safety model

`spec-elf` is a launcher and packer, not a sandbox.

A packed file contains native executable payloads. Only run packed executables from sources you trust, exactly as you would with any other native binary.

## Documentation

- [`docs/format.md`](docs/format.md): packed executable layout
- [`docs/builders.md`](docs/builders.md): builder behavior for C, C++, Rust, and Zig
