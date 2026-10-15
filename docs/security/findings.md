# Security Findings — Reference Product

> Observable security posture of the reference product's binaries. We list
> these to be **explicit about what we will NOT replicate**.

## ASLR / DEP / CFG audit

We ran `pefile` against all 535 PE files in the merged payload and checked
the `DllCharacteristics` field for:

| Flag | Bit | Meaning |
|---|---|---|
| `DYNAMIC_BASE` | 0x0040 | ASLR (Address Space Layout Randomization) |
| `NX_COMPAT` | 0x0100 | DEP / NX (Data Execution Prevention) |
| `GUARD_CF` | 0x4000 | CFG (Control Flow Guard) |
| `FORCE_INTegrity` | 0x0080 | Code integrity checks |
| `NO_SEH` | 0x0400 | No structured exception handlers |

### Findings

| Binary | ASLR | DEP | CFG | Notes |
|---|---|---|---|---|
| `DRW.exe` (main launcher, 124 KB) | **N** | Y | N | **No ASLR on the launcher.** Unusual and weak. |
| `DRWReport.exe` (4.6 MB) | **N** | Y | N | Same. |
| `DRWUI.exe` (29 MB Qt5 UI) | Y | Y | N | OK but no CFG. |
| `DRWService.exe` (39 KB privileged) | Y | Y | N | OK but no CFG. |
| All `*SearchDLL.dll` (FS parsers) | Y | Y | N | OK but no CFG. |
| `PE_Tools.dll` (installs kernel driver) | Y | Y | N | OK but no CFG. |
| Most Qt5 / OpenSSL / VC++ runtime DLLs | Y | Y | N or Y | Microsoft-built DLLs (`ucrtbase.dll`, `msvcp140.dll`) have CFG; Qt DLLs do not. |

### What this means

- **The launcher (`DRW.exe`) has no ASLR.** An attacker who can hijack the
  launcher's startup (e.g. via a DLL planting attack in the install
  directory) can rely on statically-known addresses for gadgets. This is a
  real weakness, especially because the launcher runs every time the user
  opens the app.
- **No CFG anywhere in the product's own binaries.** CFG makes
  return-oriented-programming (ROP) attacks harder by validating indirect
  call targets at runtime. Microsoft-built DLLs have it; the product's
  own DLLs do not.
- **The privileged service has ASLR but no CFG.** Better than the
  launcher, but still missing a mitigation.

### Why we will NOT replicate this

Our Rust workspace has:

```toml
# Cargo.toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = "symbols"
panic = "abort"
```

Plus, on Windows, the Rust toolchain emits binaries with ASLR, DEP, and
CFG enabled by default. We don't have to do anything special — every
binary we ship will have all three.

We also set `RUSTFLAGS="-C link-args=/guard:cfg /C link-args=/dynamicbase /C link-args=/nxcompat"`
in the CI workflow as a belt-and-suspenders measure, and we'll add a test
that runs `winchecksec` on the built binary to verify the flags are
present.

## Other observations

### Driver installation

`PE_Tools.dll` exports `pet_install_driver`. This is used to install the
custom kernel drivers (`EUDCPEPM.sys`, `EUEDKEPM.sys`, `ebrntdrv.sys`,
`epmdkdrv.sys`) for the WinPE rescue environment. The drivers themselves
are signed (we did NOT verify the signing chain — that would cross into
bypass research).

For our product: we don't need a custom kernel driver for normal
operation. Windows DeviceIoControl with `FSCTL_ALLOW_EXTENDED_DASD_IO`
gives us all the raw disk access we need from a userspace process. We
will only need a driver for the bootable rescue environment, which is a
later milestone.

### Telemetry

The reference product bundles:

- `firebasefetch.exe`, `firebaseonlineconfig.dll` — Firebase remote config
  & analytics.
- `google-services.json` — Firebase project configuration (project ID,
  API key — these are publicly visible in the bundle).
- `AliyunWrap.dll`, `AliyunConfig.ini`, `AliyunWrapExe.exe` — Aliyun OSS
  (Chinese cloud storage) integration.
- `send_dmp_with_aliyun.exe` — crash dump uploader via Aliyun.
- `remote_config_data/`, `dlconfig.xml` — remote feature flags.
- `EuUserRate.dll` — user rating prompt.
- `EUinApp.exe` — in-app messaging / upgrade prompts.

We do NOT know what data is sent. We do NOT investigate. We simply note
that the product bundles cloud-telemetry capability.

For our product: **no telemetry by default**. Opt-in crash reporting
only, with explicit consent and a clear privacy policy. No remote config.
No "in-app messaging" prompts. No upgrade upsells.

### DLL planting risk

The reference product installs into `C:\Program Files\EaseUS\...` (or
similar). The install directory contains many DLLs alongside the main
executable. If the install directory is writable by a non-admin user
(bad install practice), this is a DLL planting vector.

For our product: install to `C:\Program Files\Data Wizard\` with proper
ACLs (admin-writable only). Verify the ACLs in the installer.

### Code signing

We did NOT verify whether the reference product's binaries are signed.
For our product: we will sign the release builds with a code-signing
certificate. The certificate will be obtained in a follow-up — for the
initial scaffold, builds are unsigned (Windows SmartScreen will warn).
