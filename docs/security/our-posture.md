# Our Security Posture

## Memory safety

The entire core is in **Rust** (edition 2024). All crates have
`#![deny(unsafe_code)]` except `dw-disk`'s Windows module, which uses
`#![allow(unsafe_code)]` only inside `windows_raw.rs` (the FFI shim for
`CreateFileW` / `ReadFile` / `DeviceIoControl` — unavoidable FFI).

```rust
// crates/dw-disk/src/lib.rs
#![deny(unsafe_code)]

// crates/dw-disk/src/windows_raw.rs
#![cfg(windows)]
#![allow(unsafe_code)]  // FFI shim, audited
```

Every other crate is fully memory-safe. There is no `unsafe` in
`dw-core`, `dw-ntfs`, `dw-fat`, `dw-exfat`, `dw-ext`, `dw-hfs`,
`dw-btrfs`, `dw-refs`, `dw-zfs`, `dw-signatures`, `dw-carver`, or
`dw-ipc`.

## Binary hardening (Windows)

Every binary we ship has:

| Mitigation | How | Verified by |
|---|---|---|
| ASLR (`DYNAMIC_BASE`) | Rust default | `winchecksec` in CI |
| DEP / NX (`NX_COMPAT`) | Rust default | `winchecksec` in CI |
| CFG (`GUARD_CF`) | Rust `link-args=/guard:cfg` | `winchecksec` in CI |
| Stack cookies | Rust default | `winchecksec` in CI |
| High-entropy VA | Rust default (x86_64) | `winchecksec` in CI |

The Security Scan workflow (this repo) runs `winchecksec` on the built
`dw-tauri.exe` and fails the build if any mitigation is missing.

## Supply chain

- **Cargo**: `Cargo.lock` is committed. Dependencies are pinned to
  semver-compatible versions. `cargo audit` runs in CI.
- **npm**: `package-lock.json` is committed. `npm audit` runs in CI.
- **No third-party proprietary code** is bundled. All crates are from
  crates.io. All npm packages are from npmjs.com.
- **No telemetry SDKs** are bundled. No Firebase, no Aliyun, no
  Sentry, no analytics. Crash reports are opt-in and user-initiated
  (saved to a file the user can manually share).

## Sandboxing

Tauri 2's WebView2 runs in its own sandbox (the same sandbox as
Microsoft Edge). The Rust core runs in the Tauri process, which has the
same privilege as the user. Raw disk access requires UAC elevation
(prompted only when the user picks a physical drive).

The UI cannot:
- Execute arbitrary shell commands (no `shell` plugin exposure beyond
  `opener`).
- Read arbitrary files (the `fs` plugin is scoped to the destination
  directory the user picks).
- Make network requests (no `http` plugin).

## Data handling

- **No data leaves the user's machine** by default.
- Scan results are kept in memory; spilling to disk happens only in the
  temp directory and is cleaned up on app exit.
- Recovered files are written to the destination directory the user
  picks. We never write to the source disk.
- Logs are written to `%LOCALAPPDATA%\Data Wizard\logs\` and rotated
  daily. Logs contain no PII — only scan IDs, phases, and error
  messages.

## Threat model

| Threat | Mitigation |
|---|---|
| Malicious disk image crafted to crash the parser | Rust memory safety + fuzz tests on every parser |
| Malicious disk image crafted to RCE | No `unsafe` in parsers; no `eval`; no `shell.execute` exposure |
| DLL planting in install directory | Installer sets proper ACLs; docs warn against installing to a user-writable dir |
| Tampering with scan results on disk | Spill file written with `PROTECTION_LEVEL_USER` ACL; not yet integrity-checked — TODO |
| Reverse-engineering our binary to find a vuln | All mitigations on; bug bounty planned; responsible disclosure process in SECURITY.md |
| Attacker with physical access to the user's machine | Out of scope — use BitLocker |

## Fuzzing

Every FS parser crate has a `cargo fuzz` target:

- `dw-ntfs/fuzz/fuzz_mft_record` — fuzz the MFT record parser with
  random 1024-byte buffers.
- `dw-fat/fuzz/fuzz_dir_entry` — fuzz the FAT directory entry parser.
- `dw-exfat/fuzz/fuzz_dir_entry` — fuzz the exFAT directory entry parser.
- `dw-ext/fuzz/fuzz_inode` — fuzz the ext4 inode parser.
- etc.

Fuzz targets are run in CI on every PR (short runs) and nightly (long
runs). Corpus is committed to the repo so regressions are caught.

## Responsible disclosure

See [`SECURITY.md`](../../SECURITY.md) for the responsible disclosure
process. TL;DR: email security@datawizard.io (to be set up), expect a
response within 72 hours, do not publish until we've shipped a fix.
