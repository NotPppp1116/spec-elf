# Packed executable format

This document describes the current `spec-elf` packed-file layout.

The format is intentionally simple: payloads are appended to a normal launcher executable, then a manifest and fixed-size footer are appended at the end.

## High-level layout

```text
+--------------------------+
| launcher executable      |
+--------------------------+
| payload 0 bytes          |
+--------------------------+
| payload 1 bytes          |
+--------------------------+
| ...                      |
+--------------------------+
| manifest                 |
+--------------------------+
| footer                   |
+--------------------------+
```

The launcher remains at the start of the file, so the operating system can still execute the packed file normally.

At runtime, the launcher opens its own executable, reads the footer from the end, then uses the footer to find the manifest and payload byte ranges.

## Manifest layout

All integer fields are little-endian.

```text
u32 entry_count

repeated entry_count times:
    u32 name_len
    u8[name_len] name_utf8
    u64 payload_offset
    u64 payload_size
```

Each manifest entry names one payload and stores the byte range of that payload inside the packed executable.

`payload_offset` is an absolute byte offset from the beginning of the packed file.

`payload_size` is the exact number of bytes to read for that payload.

## Footer layout

The footer is fixed-size and is always the last 33 bytes of the packed file.

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

The current launch flag value is `1`.

A file is treated as a packed archive only when:

1. the file is large enough to contain the footer
2. the footer magic matches
3. the launch flag is `1`

## Native hash

The `native_hash` field is used to decide whether the `native` payload is safe to reuse on the current machine.

When packing, `spec-elf` computes a hash from CPU and target-platform information.

When launching, it recomputes that hash. If the stored hash matches and a payload name contains `native`, that payload is selected.

If the hash does not match, `spec-elf` falls back to x86-64 level selection.

## x86-64 payload selection

If the native payload is not selected, the launcher detects the current x86-64 level:

```text
x86-64
x86-64-v2
x86-64-v3
x86-64-v4
```

It then searches the manifest for a payload whose name matches or ends with the detected level.

Examples of names that can match:

```text
c-x86-64-v3
cpp-x86-64-v3
rust-x86_64_v3
zig-x86_64_v3
```

## Important invariants

The writer must ensure:

- payload offsets point inside the packed file
- payload sizes do not run past the manifest/footer
- the manifest is written after all payloads
- the footer is written last
- the footer size stays in sync with the reader

The reader must reject:

- files smaller than the footer
- files with invalid magic
- files where `manifest_offset + manifest_size` points outside the payload/manifest area
- invalid UTF-8 payload names
- missing compatible payloads

## Stability

This format is not stable yet. It is fine to break compatibility while the project is still experimental, but changes should update this document and the reader/writer together.
