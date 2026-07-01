# Code map

This document is a quick guide for people reading the `spec-elf` source for the first time.

`spec-elf` has two main modes:

1. **Build/pack mode**: compile the target project for several CPU targets and pack the resulting binaries behind the launcher.
2. **Archive/runtime mode**: when the launcher detects that it is already packed, extract the best payload and run it.

## Entry point

`src/main.rs` decides which mode is active.

At startup it asks `is_archive(current_exe)`:

- `false`: build payload binaries, pack them, and write the packed launcher into the target project directory
- `true`: read the packed payload list, choose the best binary for the current machine, replace the packed file with that payload, and start the replacement

The `--dir <path>` argument changes the target project directory before language detection starts. The `-ct <levels...>` argument narrows which CPU target payloads are built.

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
2. Otherwise, detect the current x86-64 level and search downward for the best compatible portable payload.

Current x86-64 levels:

```text
x86-64
x86-64-v2
x86-64-v3
x86-64-v4
```

Rust and Zig payload names use underscores in some places, so the matcher accepts both dash and underscore variants.

The portable fallback never chooses a payload above the detected CPU level.

## Builders

`src/builder/compile.rs` detects the dominant language in the target directory and calls the matching builder.

Supported builders:

- C: `gcc`, or CMake when `CMakeLists.txt` exists
- C++: `g++`, or CMake when `CMakeLists.txt` exists
- Rust: `cargo build --release` with per-target `RUSTFLAGS`
- Zig: `zig build-exe -O ReleaseFast`

The builder writes final payloads into the target project's `build/` directory.

By default, all supported CPU levels are built. `-ct` can select a subset such as `base v2 v3` or `native`.

## CPU detection

`src/arch/x86.rs` contains CPU feature detection.

`detect_x86_level` returns the highest x86-64 level supported by the current CPU.

`native_hasher` creates a hash from CPU/platform details. The hash is used only to decide whether a `native` build was produced for the same kind of machine that is now launching the archive.

## Tests

The archive tests in `src/archive/format.rs` use temporary files to test the real reader/writer path:

- a normal file is not detected as an archive
- a packed file is detected as an archive
- a packed file can be read back into the selected payload bytes
- portable fallback chooses the best lower compatible x86-64 payload
- self-replacement writes the selected payload back to the executable path
- CLI target-level parsing accepts valid selections and rejects empty `-ct`

These tests are useful because the file format depends on real offsets, seeks, and byte sizes.
