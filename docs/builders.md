# Builder behavior

`spec-elf` builds several payload binaries for a target project, then packs them into the launcher.

The builder lives in `src/builder/compile.rs`.

## Language detection

The target project language is chosen by recursively counting file extensions.

Ignored directories:

```text
target
build
.git
```

Current extension mapping:

| Extension | Language |
| --- | --- |
| `.c`, `.h` | C |
| `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hxx` | C++ |
| `.rs` | Rust |
| `.zig` | Zig |

The language with the highest count is selected.

## C projects

If the project has a `CMakeLists.txt`, `spec-elf` uses CMake.

Otherwise it directly invokes `gcc` over all collected `.c` files.

Manual GCC builds add these include directories:

```text
-Iinclude
-Isrc
```

C payload output names:

```text
build/c-native
build/c-x86-64
build/c-x86-64-v2
build/c-x86-64-v3
build/c-x86-64-v4
```

## C++ projects

If the project has a `CMakeLists.txt`, `spec-elf` uses CMake.

Otherwise it directly invokes `g++` over all collected `.cpp`, `.cc`, and `.cxx` files.

Manual G++ builds add these include directories:

```text
-I.
-Iinclude
-Isrc
```

C++ payload output names:

```text
build/cpp-native
build/cpp-x86-64
build/cpp-x86-64-v2
build/cpp-x86-64-v3
build/cpp-x86-64-v4
```

## Rust projects

Rust projects are built with Cargo.

For each CPU target, `spec-elf` sets:

- `RUSTFLAGS`
- `CARGO_TARGET_DIR`

This keeps the target builds separate.

Rust payload output names:

```text
build/rust-native
build/rust-x86_64
build/rust-x86_64_v2
build/rust-x86_64_v3
build/rust-x86_64_v4
```

## Zig projects

Zig projects currently build the first discovered `.zig` source file with:

```text
zig build-exe -O ReleaseFast
```

Zig payload output names:

```text
build/zig-native
build/zig-x86_64
build/zig-x86_64_v2
build/zig-x86_64_v3
build/zig-x86_64_v4
```

## CMake behavior

For C and C++, CMake builds use one build directory per CPU target.

The runtime output directory is redirected into a per-target CMake output directory, then `spec-elf` copies the single executable it finds into the stable payload path.

Current limitation: if CMake produces multiple executables for the same target, the build fails because `spec-elf` does not yet know which executable should become the payload.

## Things to improve next

Useful next improvements:

- add explicit CLI flags for language selection
- add explicit CLI flags for the executable target name
- add a config file instead of relying only on extension counting
- add fallback target selection when the exact x86-64 level payload is missing
- add tests around payload naming and selection
