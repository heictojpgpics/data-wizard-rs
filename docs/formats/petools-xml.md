# PEtools.xml — Format Reference

> **Format only.** Documents the XML schema used by the reference
> product's `PE_Tools.dll` to bootstrap a WinPE bootable rescue
> environment. We document this for completeness; **we do NOT
> implement a WinPE rescue environment in v0.1** (it's a later
> milestone, see `docs/architecture/process-model.md`).

## Purpose

`PEtools.xml` is a manifest of Windows AIK (Automated Installation Kit)
/ ADK (Assessment and Deployment Kit) tools that `PE_Tools.dll`
downloads, verifies, and installs into a target directory to build a
WinPE boot image. The exports of `PE_Tools.dll` are:

- `pet_install_driver` — install a kernel driver
- `pet_download_packet` — download a CAB packet from a server
- `pet_decompress_packet` — decompress a CAB
- `pet_verify_packet` — verify a CAB's signature/checksum
- `pet_is_exist_packet` — check if a packet is already downloaded
- `pet_set_download_path` — set the download cache directory
- `pet_get_petools_dir` — get the PETools working directory
- `pet_regist_progress_fun` — register a progress callback
- `pet_regist_status_fun` — register a status callback
- `pet_set_progress_data` — push progress data to the callback

## Schema

```xml
<?xml version="1.0"?>
<!-- Encode = ANSI -->
<PETOOLS>
  <WINPE>
    <CAB name="Tools_1" output="\Windows AIK\Tools">
      <FILE name="x86_bcdboot.exe" new="x86\bcdboot.exe"/>
      <FILE name="F1_WIMMOUNTINF"  new="x86\wimmount.inf"/>
      <FILE name="F1_WIMMOUNT"     new="x86\wimmount.sys"/>
      <!-- ... more FILE entries ... -->
    </CAB>
    <CAB name="Tools_2" output="\Windows AIK\Tools">
      <!-- ... -->
    </CAB>
    <!-- ... more CAB entries ... -->
  </WINPE>
</PETOOLS>
```

## Elements

| Element | Attributes | Purpose |
|---|---|---|
| `PETOOLS` (root) | — | Top-level container. |
| `WINPE` | — | Groups all WinPE-related tool packets. |
| `CAB` | `name`, `output` | A single CAB packet. `name` is the packet identifier (downloaded as `<name>.cab`); `output` is the target directory the CAB's contents are extracted to. |
| `FILE` | `name`, `new` | A file mapping. `name` is the source filename inside the CAB; `new` is the destination path relative to `CAB.output`. |

## Our approach (later milestone)

When we implement Crashed PC Rescue (post-v0.1), we'll need an
equivalent bootable rescue image. Our approach will be:

1. **Don't bundle Microsoft's AIK/ADK tools.** Those are redistributable
   only under a Microsoft license. Instead, we'll document how the user
   can install the Windows ADK themselves and point our builder at it.
2. **Use WinPE or WinRE as the base.** WinRE is already present on
   every Windows 10/11 install (in `C:\Recovery\`); we can extract it
   and customize it.
3. **Our binary is the only thing we add.** The rescue image will
   contain our `dw-tauri.exe` plus the minimum needed to boot WinPE,
   open raw disk handles, and run a scan.

This is a significant later milestone and is NOT in scope for v0.1.
