# Code map

This document is a quick guide for people reading the `spec-elf` source for the first time.

`spec-elf` has two main modes:

1. **Build/pack mode**: compile the target project for several CPU targets and pack the resulting binaries behind the launcher.
2. **Archive/runtime mode**: when the launcher detects that it is already packed, extract the best payload and run it.

## Entry point

`src/main.rs` decides which mode is active.

At startup it asks `is_archive(current_exe)`:

- `false`: build payload binaries, pack them, and write the packed launcher into the target project directory
- `true`: read the packed payload list, choose the best binary for the current machine, write it to disk, and spawn it

The `--dir <path>` argument only changes the target project directory before language detection starts.

## Archive format

`src/archive/format.rs` owns the packed file reader and writer.

The writer layout is:

```text
[launcher executable]
[payload bytes]
[manifest]
[footer]
```

The footer is fixed-size and stored at the very end of the file, so the reader can find it with one seek from EOF.

The manifest stores one entry per payload:

```text
name
payload offset
payload size
```

The payload offset is absolute from the start of the packed file.

## Payload selection

Payload selection is handled by `find_optimal` in `src/archive/format.rs`.

Selection order:

1. If the stored native CPU hash matches the current machine, use a payload whose name contains `native`.
2. Otherwise, detect the current x86-64 level and search for a matching payload name.

Current x86-64 levels:

```text
x86-64
x86-64-v2
x86-64-v3
x86-64-v4
```

Rust and Zig payload names use underscores in some places, so the matcher accepts both dash and underscore variants.

## Builders

`src/builder/compile.rs` detects the dominant language in the target directory and calls the matching builder.

Supported builders:

- C: `gcc`, or CMake when `CMakeLists.txt` exists
- C++: `g++`, or CMake when `CMakeLists.txt` exists
- Rust: `cargo build --release` with per-target `RUSTFLAGS`
- Zig: `zig build-exe -O ReleaseFast`

The builder writes final payloads into the target project's `build/` directory.

## CPU detection

`src/arch/x86.rs` contains CPU feature detection.

`detect_x86_level` returns the highest x86-64 level supported by the current CPU.

`native_hasher` creates a hash from CPU/platform details. The hash is used only to decide whether a `native` build was produced for the same kind of machine that is now launching the archive.

## Tests

The archive tests in `src/archive/format.rs` use temporary files to test the real reader/writer path:

- a normal file is not detected as an archive
- a packed file is detected as an archive
- a packed file can be read back into the selected payload bytes

These tests are useful because the file format depends on real offsets, seeks, and byte sizes.
