#!/usr/bin/env python3
"""
Build a machine-readable file inventory of the merged EaseUS payload.

For each file we record:
  - relative path, size, mtime
  - file type (extension + magic)
  - sha256 (for de-duplication and tracking)
  - for PE files: machine, subsystem, sections, imports (dll -> func list),
    exports (function names — these are PUBLIC API surface, observable facts),
    version-info strings, security flags (ASLR/DEP/SafeSEH/CFG/NX)

Output:
  - /home/z/my-project/analysis/reports/inventory.json   (full machine record)
  - /home/z/my-project/analysis/reports/inventory.md     (human-readable summary)

We do NOT dump decompiled code, raw strings, or proprietary asset bytes.
We DO record observable structural facts (PE exports, imports, sections, flags).
"""
from __future__ import annotations
import os
import sys
import json
import hashlib
import datetime
from pathlib import Path

import pefile
import lief

ROOT = Path("/home/z/my-project/analysis/extracted/merged")
OUT_JSON = Path("/home/z/my-project/analysis/reports/inventory.json")
OUT_MD = Path("/home/z/my-project/analysis/reports/inventory.md")

# ---------- helpers ----------

def sha256_of(path: Path, limit_mb: int = 256) -> str:
    h = hashlib.sha256()
    size = path.stat().st_size
    with path.open("rb") as f:
        remaining = min(size, limit_mb * 1024 * 1024)
        while remaining > 0:
            chunk = f.read(min(1 << 20, remaining))
            if not chunk:
                break
            h.update(chunk)
            remaining -= len(chunk)
    return h.hexdigest()

def classify_by_ext(name: str) -> str:
    n = name.lower()
    if n.endswith((".dll", ".exe", ".sys", ".ocx", ".scr")):
        return "pe"
    if n.endswith((".json",)):
        return "json"
    if n.endswith((".xml",)):
        return "xml"
    if n.endswith((".ini", ".cfg", ".conf")):
        return "ini"
    if n.endswith((".txt", ".log", ".manifest")):
        return "text"
    if n.endswith((".cat", ".cer", ".crt", ".p7b")):
        return "cert"
    if n.endswith((".ttf", ".ttc", ".otf")):
        return "font"
    if n.endswith((".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".bmp")):
        return "image"
    if n.endswith((".qm",)):
        return "qt-translation"
    if n.endswith((".qss", ".css")):
        return "stylesheet"
    if n.endswith((".dat", ".bin", ".db", ".sqlite")):
        return "binary-data"
    if n.endswith((".so", ".dylib")):
        return "unix-lib"
    return "other"

def detect_magic(path: Path) -> str:
    try:
        with path.open("rb") as f:
            head = f.read(512)
    except OSError:
        return "unreadable"
    if head[:2] == b"MZ":
        return "pe-maybe"
    if head[:4] == b"\x7fELF":
        return "elf"
    if head[:4] in (b"\xfe\xed\xfa\xce", b"\xfe\xed\xfa\xcf",
                    b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe"):
        return "mach-o"
    if head[:4] == b"PK\x03\x04":
        return "zip"
    if head[:4] == b"\x89PNG":
        return "png"
    if head[:3] == b"\xff\xd8\xff":
        return "jpeg"
    if head[:4] == b"GIF8":
        return "gif"
    if head[:2] == b"BM":
        return "bmp"
    if head[:5] == b"<?xml":
        return "xml-text"
    if head[:1] == b"{":
        return "json-maybe"
    if head[:4] == b"\xff\xfe\x00\x00" or head[:4] == b"\x00\x00\xfe\xff":
        return "utf-32-bom"
    if head[:2] == b"\xff\xfe" or head[:2] == b"\xfe\xff":
        return "utf-16-bom"
    if b"\x00" in head[:32]:
        return "binary"
    try:
        head.decode("utf-8")
        return "text-utf8"
    except UnicodeDecodeError:
        return "binary"

# ---------- PE analysis ----------

def analyze_pe(path: Path) -> dict:
    info = {}
    try:
        pe = pefile.PE(str(path), fast_load=True)
    except Exception as e:
        info["error"] = f"pefile-load: {e!r}"
        return info

    # machine / subsystem
    machine = pe.FILE_HEADER.Machine
    info["machine"] = {
        0x014c: "i386",
        0x8664: "amd64",
        0x01c0: "arm",
        0xaa64: "arm64",
    }.get(machine, f"unknown-0x{machine:x}")
    info["is_64bit"] = pe.OPTIONAL_HEADER.Magic == 0x20b
    info["subsystem"] = pe.OPTIONAL_HEADER.Subsystem
    info["subsystem_name"] = {
        2: "GUI",
        3: "Console",
        9: "WindowsCE",
    }.get(pe.OPTIONAL_HEADER.Subsystem, f"unknown-{pe.OPTIONAL_HEADER.Subsystem}")
    info["entrypoint_rva"] = pe.OPTIONAL_HEADER.AddressOfEntryPoint
    info["image_size"] = pe.OPTIONAL_HEADER.SizeOfImage
    info["timestamp"] = datetime.datetime.utcfromtimestamp(
        pe.FILE_HEADER.TimeDateStamp).isoformat() + "Z" if pe.FILE_HEADER.TimeDateStamp else None
    info["num_sections"] = pe.FILE_HEADER.NumberOfSections

    # sections
    info["sections"] = []
    for s in pe.sections:
        info["sections"].append({
            "name": s.Name.rstrip(b"\x00").decode("latin1", "replace"),
            "vsize": s.Misc_VirtualSize,
            "rsize": s.SizeOfRawData,
            "entropy": round(s.get_entropy(), 3),
        })

    # security flags (DLL characteristics)
    dc = pe.OPTIONAL_HEADER.DllCharacteristics
    info["sec_flags"] = {
        "aslr":  bool(dc & 0x0040),  # DYNAMIC_BASE
        "nx":    bool(dc & 0x0100),  # NX_COMPAT
        "force_integrity": bool(dc & 0x0080),
        "no_seh": bool(dc & 0x0400),
        "cfg":   bool(dc & 0x4000),  # GUARD_CF
        "terminal_server_aware": bool(dc & 0x8000),
    }

    # imports (dll -> function list, public API surface observation)
    try:
        pe.parse_data_directories()
    except Exception:
        pass
    imports = {}
    if hasattr(pe, "DIRECTORY_ENTRY_IMPORT"):
        for entry in pe.DIRECTORY_ENTRY_IMPORT:
            dll = entry.dll.decode("latin1", "replace").lower()
            funcs = []
            for imp in entry.imports:
                if imp.name:
                    funcs.append(imp.name.decode("latin1", "replace"))
                elif imp.ordinal:
                    funcs.append(f"#{imp.ordinal}")
            imports[dll] = funcs
    info["imports"] = imports
    info["imports_count_dlls"] = len(imports)
    info["imports_count_funcs"] = sum(len(v) for v in imports.values())

    # exports (function names — observable public API)
    exports = []
    if hasattr(pe, "DIRECTORY_ENTRY_EXPORT"):
        for exp in pe.DIRECTORY_ENTRY_EXPORT.symbols:
            if exp.name:
                exports.append(exp.name.decode("latin1", "replace"))
            elif exp.ordinal:
                exports.append(f"#{exp.ordinal}")
    info["exports"] = exports
    info["exports_count"] = len(exports)

    # version-info strings
    info["version_info"] = {}
    if hasattr(pe, "FileInfo"):
        try:
            for finfo in pe.FileInfo:
                for entry in finfo:
                    if hasattr(entry, "StringTable"):
                        for st in entry.StringTable:
                            for k, v in st.entries.items():
                                info["version_info"][k.decode("latin1", "replace")] = v.decode("latin1", "replace")
        except Exception:
            pass

    # check for .NET (managed) via IMAGE_DIRECTORY_ENTRY_COM_DESCRIPTOR
    info["is_dotnet"] = False
    try:
        clr = pe.OPTIONAL_HEADER.DATA_DIRECTORY[14]  # COM_DESCRIPTOR
        if clr and clr.Size > 0:
            info["is_dotnet"] = True
    except Exception:
        pass

    try:
        pe.close()
    except Exception:
        pass
    return info

# ---------- main ----------

def main():
    files = []
    for dp, dirs, fns in os.walk(ROOT):
        for fn in fns:
            files.append(Path(dp) / fn)

    files.sort()
    print(f"[+] Walking {len(files)} files...")

    inventory = []
    counts = {"total": 0, "pe": 0, "json": 0, "xml": 0, "ini": 0, "text": 0,
              "image": 0, "font": 0, "binary-data": 0, "other": 0}
    pe_summary = []

    for i, p in enumerate(files, 1):
        rel = p.relative_to(ROOT).as_posix()
        try:
            st = p.stat()
            size = st.st_size
            mtime = datetime.datetime.utcfromtimestamp(st.st_mtime).isoformat() + "Z"
        except OSError:
            size = 0
            mtime = None
        ext_class = classify_by_ext(p.name)
        magic = detect_magic(p)
        record = {
            "path": rel,
            "size": size,
            "mtime": mtime,
            "ext_class": ext_class,
            "magic": magic,
        }
        counts["total"] += 1
        counts[ext_class] = counts.get(ext_class, 0) + 1

        is_pe = (magic == "pe-maybe" and ext_class == "pe")
        if is_pe:
            pe_info = analyze_pe(p)
            record["pe"] = pe_info
            counts["pe"] += 0  # already counted above
            pe_summary.append({
                "path": rel,
                "size": size,
                "machine": pe_info.get("machine"),
                "is_64bit": pe_info.get("is_64bit"),
                "subsystem": pe_info.get("subsystem_name"),
                "is_dotnet": pe_info.get("is_dotnet"),
                "sec_flags": pe_info.get("sec_flags"),
                "imports_count_dlls": pe_info.get("imports_count_dlls"),
                "imports_count_funcs": pe_info.get("imports_count_funcs"),
                "exports_count": pe_info.get("exports_count"),
                "version_info": pe_info.get("version_info"),
            })

        inventory.append(record)
        if i % 100 == 0:
            print(f"  ... {i}/{len(files)}")

    OUT_JSON.parent.mkdir(parents=True, exist_ok=True)
    with OUT_JSON.open("w") as f:
        json.dump({
            "root": str(ROOT),
            "counts": counts,
            "files": inventory,
        }, f, indent=2, default=str)
    print(f"[+] Wrote {OUT_JSON} ({OUT_JSON.stat().st_size // 1024} KB)")

    # Markdown report
    lines = []
    lines.append("# File Inventory — EaseUS Data Recovery Wizard (merged payload)")
    lines.append("")
    lines.append(f"- **Root**: `{ROOT}`")
    lines.append(f"- **Total files**: {counts['total']}")
    lines.append("")
    lines.append("## File-type counts")
    lines.append("")
    lines.append("| Class | Count |")
    lines.append("|---|---|")
    for k, v in sorted(counts.items(), key=lambda x: -x[1]):
        if k == "total":
            continue
        lines.append(f"| {k} | {v} |")
    lines.append("")

    lines.append("## PE files — summary (sorted by import-DLL count)")
    lines.append("")
    lines.append("Sortable list of all PE binaries with security flags, machine, subsystem, "
                 "import/export counts, and version-info strings (observable facts).")
    lines.append("")
    lines.append("| Path | Size (KB) | Machine | Subsystem | .NET | ASLR | NX | CFG | Imports (DLLs/funcs) | Exports | Product / Company |")
    lines.append("|---|---|---|---|---|---|---|---|---|---|---|")
    for row in sorted(pe_summary, key=lambda r: -(r.get("imports_count_dlls") or 0)):
        sec = row.get("sec_flags") or {}
        vi = row.get("version_info") or {}
        prod = (vi.get("ProductName") or vi.get("FileDescription") or "")[:40]
        comp = (vi.get("CompanyName") or "")[:30]
        lines.append(
            f"| `{row['path']}` | {row['size']//1024} | {row.get('machine')} | "
            f"{row.get('subsystem')} | {'yes' if row.get('is_dotnet') else 'no'} | "
            f"{'Y' if sec.get('aslr') else 'N'} | {'Y' if sec.get('nx') else 'N'} | "
            f"{'Y' if sec.get('cfg') else 'N'} | "
            f"{row.get('imports_count_dlls')}/{row.get('imports_count_funcs')} | "
            f"{row.get('exports_count')} | {prod} / {comp} |"
        )
    lines.append("")

    OUT_MD.write_text("\n".join(lines))
    print(f"[+] Wrote {OUT_MD} ({OUT_MD.stat().st_size // 1024} KB)")
    print(f"[+] PE files cataloged: {len(pe_summary)}")

if __name__ == "__main__":
    main()
