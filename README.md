# spec-elf

`spec-elf` is an experimental Rust launcher/packer for native executables.

It builds several versions of a project for different CPU targets, packs those binaries behind a small launcher, and later extracts/runs the binary that best matches the current machine.

The current focus is simple:

- detect the project language
- build optimized payload binaries
- append those payloads to the launcher executable
- store a small manifest/footer at the end of the file
- choose the best payload at runtime

This is still early-stage tooling. The packed format and runtime behavior are not stable yet.

## Supported inputs

`spec-elf` currently detects the dominant language in the target directory by counting source-file extensions.

Supported builders:

| Language | Detection | Build path |
| --- | --- | --- |
| C | `.c`, `.h` | `gcc`, or CMake if `CMakeLists.txt` exists |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hxx` | `g++`, or CMake if `CMakeLists.txt` exists |
| Rust | `.rs` | `cargo build --release` with per-target `RUSTFLAGS` |
| Zig | `.zig` | `zig build-exe -O ReleaseFast` |

Generated payloads are written under the target project's `build/` directory.

## CPU targets

For C/C++, `spec-elf` currently builds:

```text
-march=native
-march=x86-64
-march=x86-64-v2
-march=x86-64-v3
-march=x86-64-v4
```

For Rust, the equivalent `target-cpu` values are used:

```text
native
x86-64
x86-64-v2
x86-64-v3
x86-64-v4
```

For Zig, the equivalent `-mcpu` values are used:

```text
native
x86_64
x86_64_v2
x86_64_v3
x86_64_v4
```

## Usage

Build `spec-elf`:

```bash
cargo build --release
```

Run it from the target project directory:

```bash
/path/to/spec-elf
```

Or pass a directory explicitly:

```bash
/path/to/spec-elf --dir /path/to/project
```

The packed output is written into the target directory using the same file name as the launcher.

## Packed file format

The packed executable is laid out like this:

```text
[launcher executable bytes]
[payload 0 bytes]
[payload 1 bytes]
[...]
[manifest]
[footer]
```

The footer is fixed-size and lives at the end of the file. It points back to the manifest.

See [`docs/format.md`](docs/format.md) for the exact layout.

## Runtime selection

When a packed executable starts, the launcher checks whether its own file has a valid `spec-elf` footer.

If it is packed, it:

1. reads the footer
2. finds the manifest
3. reads the payload list
4. detects the current x86-64 CPU level
5. chooses the matching payload
6. writes the selected executable to disk
7. starts it

The `native` payload is only selected when the stored native CPU hash matches the current machine.

## Current limitations

- x86-64-focused target selection only.
- The file format is experimental.
- The launcher currently expects the packed payload set to contain a matching target.
- C/C++ manual builds are intentionally simple and may not support complex projects without CMake.
- CMake projects are supported, but projects that produce multiple executables may need more explicit selection in the future.
- This is not a security sandbox. Do not run untrusted packed binaries.

## Repository layout

```text
src/main.rs              CLI entry point and pack/archive mode switch
src/archive/format.rs    packed file writer/reader and payload selector
src/arch/x86.rs          x86-64 feature-level detection and native hash
src/builder/compile.rs   C, C++, Rust, and Zig build backends
```

## Development notes

Useful checks while working on the project:

```bash
cargo fmt
cargo clippy -- -W clippy::pedantic
cargo test
cargo build --release
```

There are no formal tests yet. The next useful step is adding small tests for the manifest/footer reader and writer.
