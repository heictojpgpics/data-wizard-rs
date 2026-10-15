#!/usr/bin/env python3
"""
Extract PUBLIC API surface of the core EaseUS modules.

We only record the EXPORT NAMES of the core DLLs (these are observable function
names that any process loading the DLL would see — they are public API surface,
not decompiled code). We also record the import DLL names (not function lists
for non-core modules — only for our own modules).

For each core DLL, we also record:
  - section list
  - whether it imports any FS-specific Win32 APIs (DeviceIoControl, CreateFile, etc.)
  - whether it imports any crypto/registry/network APIs (interesting behavioral signal)

We DO NOT dump:
  - raw bytes
  - string tables
  - decompiled pseudocode
  - proprietary resource contents
"""
from __future__ import annotations
import json
from pathlib import Path
import pefile

ROOT = Path("/home/z/my-project/analysis/extracted/merged")
OUT = Path("/home/z/my-project/analysis/reports/core_api_surface.md")

# Core modules whose public API surface is relevant to our reverse-engineering
CORE_DLLS = [
    "DRW.exe",
    "DRWUI.exe",
    "DRWService.exe",
    "DRWReport.exe",
    "NTFSSearchDLL.dll",
    "FATSearchDLL.dll",
    "exFATSearchDLL.dll",
    "EXT4SearchDLL.dll",
    "HFSPlusSearchDLL.dll",
    "BtrfsSearchDLL.dll",
    "ReFSSearchDLL.dll",
    "ZfsSearchDLL.dll",
    "RAWSearchDLL.dll",
    "PE_Tools.dll",
    "EUinApp.exe",
    "EuRepairHost.exe",
    "InfoForSetup.exe",
    "legacy.dll",
    "archive.dll",
    "cloud_session.dll",
    "AliyunWrap.dll",
    "EuICloudRecovery.dll",
    "firebaseonlineconfig.dll",
    "PubLog.dll",
    "EUHwpFilePathCheckerModule.dll",
    "EUVideoRepair.dll",
    "EUImageEnhance.dll",
    "EuUserRate.dll",
    "Japanese.dll",
]

# Behaviorally-interesting Win32 API substrings to flag
INTERESTING_API_MARKERS = {
    "raw_disk":   ["DeviceIoControl", "CreateFileW", "CreateFileA", "NtOpenFile", "NtCreateFile", "NtReadFile", "NtWriteFile", "ZwOpenFile"],
    "fs":         ["GetVolumeInformation", "GetLogicalDrives", "FindFirstFile", "FindNextFile", "GetDiskFreeSpace"],
    "memory":     ["VirtualAlloc", "VirtualProtect", "WriteProcessMemory", "ReadProcessMemory", "CreateRemoteThread"],
    "registry":   ["RegOpenKey", "RegQueryValue", "RegSetValue", "RegCreateKey"],
    "service":    ["OpenSCManager", "CreateService", "StartService", "ControlService"],
    "process":    ["CreateProcess", "ShellExecute", "WinExec"],
    "net":        ["WinHttpOpen", "InternetOpen", "WSAStartup", "socket", "connect"],
    "crypto":     ["CryptAcquireContext", "BCryptOpen", "CryptDecrypt", "CertOpenStore"],
    "driver":     ["SCManager", "NtLoadDriver", "NtUnloadDriver", "ZwLoadDriver"],
    "pipe_ipc":   ["CreateNamedPipe", "ConnectNamedPipe", "CallNamedPipe", "CreatePipe"],
    "elevate":    ["OpenProcessToken", "LookupPrivilegeValue", "AdjustTokenPrivileges"],
}

def analyze(path: Path) -> dict:
    pe = pefile.PE(str(path), fast_load=False)
    # exports
    exports = []
    if hasattr(pe, "DIRECTORY_ENTRY_EXPORT"):
        for exp in pe.DIRECTORY_ENTRY_EXPORT.symbols:
            if exp.name:
                exports.append(exp.name.decode("latin1", "replace"))
            elif exp.ordinal:
                exports.append(f"#{exp.ordinal}")
    # imports
    imports = {}
    api_set = set()
    if hasattr(pe, "DIRECTORY_ENTRY_IMPORT"):
        for entry in pe.DIRECTORY_ENTRY_IMPORT:
            dll = entry.dll.decode("latin1", "replace").lower()
            funcs = []
            for imp in entry.imports:
                if imp.name:
                    fn = imp.name.decode("latin1", "replace")
                    funcs.append(fn)
                    api_set.add(fn)
                elif imp.ordinal:
                    funcs.append(f"#{imp.ordinal}")
            imports[dll] = funcs
    # behavior flags
    behavior = {}
    for cat, markers in INTERESTING_API_MARKERS.items():
        hits = sorted({m for m in markers if any(m in fn for fn in api_set)})
        if hits:
            behavior[cat] = hits
    info = {
        "path": str(path.relative_to(ROOT)),
        "size": path.stat().st_size,
        "exports": exports,
        "imports": imports,
        "behavior_signals": behavior,
    }
    pe.close()
    return info

def main():
    out = ["# Core Modules — Public API Surface", "",
           "Observable facts (PE export names, import DLLs, behavioral API markers).",
           "We DO NOT reproduce decompiled code or proprietary strings.", ""]
    for name in CORE_DLLS:
        p = ROOT / name
        if not p.exists():
            out.append(f"## `{name}` — NOT FOUND")
            out.append("")
            continue
        try:
            info = analyze(p)
        except Exception as e:
            out.append(f"## `{name}` — ERROR: {e!r}")
            out.append("")
            continue
        out.append(f"## `{name}` ({info['size']:,} bytes)")
        out.append("")
        out.append(f"- **Exports**: {len(info['exports'])}")
        out.append(f"- **Import DLLs**: {len(info['imports'])}")
        out.append(f"- **Import functions (total)**: {sum(len(v) for v in info['imports'].values())}")
        out.append("")
        if info["exports"]:
            out.append("### Exports (public API)")
            out.append("")
            for e in info["exports"][:200]:
                out.append(f"- `{e}`")
            if len(info["exports"]) > 200:
                out.append(f"- ... and {len(info['exports']) - 200} more (see inventory.json)")
            out.append("")
        if info["behavior_signals"]:
            out.append("### Behavioral signals (Win32 API usage)")
            out.append("")
            for cat, hits in info["behavior_signals"].items():
                out.append(f"- **{cat}**: {', '.join(f'`{h}`' for h in hits)}")
            out.append("")
        if info["exports"]:
            # show import DLL list (not function lists) for compactness
            out.append("### Import DLLs")
            out.append("")
            for dll in sorted(info["imports"].keys()):
                out.append(f"- `{dll}` ({len(info['imports'][dll])} funcs)")
            out.append("")
        out.append("---")
        out.append("")
    OUT.write_text("\n".join(out))
    print(f"[+] Wrote {OUT} ({OUT.stat().st_size // 1024} KB)")

if __name__ == "__main__":
    main()
