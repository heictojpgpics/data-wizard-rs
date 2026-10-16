#!/usr/bin/env python3
"""Qt RCC (qres v3) full extractor — exact format per qtbase rcc.cpp.

Layout (Binary, formatVersion 3):
  header 24 bytes: 'qres' + [ver u32][treeOff u32][dataOff u32][namesOff u32][overallFlags u32]
  blobs:  repeated [size u32][payload]  (payload may be zlib/zstd per node flags)
  names:  repeated [len u16][hash u32][len × u16-BE chars]
  tree:   repeated 22-byte nodes, 0-based numbering, root = node 0
          dir : [nameOff u32][flags u16][childCount u32][childOff u32][lastmod u64]
          file: [nameOff u32][flags u16][territory u16][language u16][dataOff u32][lastmod u64]
  node.dataOff points at the [size] field of its blob, relative to blob region start.
"""
import struct, sys, os, json, zlib

FLAG_DIR = 0x02
FLAG_COMPRESSED = 0x04
FLAG_ZSTD = 0x08
NODE = 22

def parse_names(data, off):
    names = []
    pos = off
    while pos + 6 <= len(data):
        ln = struct.unpack('>H', data[pos:pos+2])[0]
        if ln == 0 or ln > 512:
            break
        raw = data[pos+6:pos+6+ln*2]
        if len(raw) < ln*2:
            break
        try:
            s = raw.decode('utf-16-be')
        except UnicodeDecodeError:
            break
        if not all(c.isprintable() for c in s):
            break
        names.append(s)
        pos += 6 + ln*2
    return names

def main():
    src, outdir = sys.argv[1], sys.argv[2]
    os.makedirs(outdir, exist_ok=True)
    data = open(src, 'rb').read()
    assert data[:4] == b'qres', "not an rcc"
    version, treeOff, dataOff, namesOff, overallFlags = struct.unpack('>5I', data[4:24])
    print(f"v{version} overallFlags={overallFlags} data@{dataOff:#x} names@{namesOff:#x} tree@{treeOff:#x}")

    # names: nameOff in nodes is a BYTE OFFSET into this region
    def read_name_at(off):
        if off + 6 > len(data) - namesOff:
            return None
        ln = struct.unpack('>H', data[namesOff+off:namesOff+off+2])[0]
        if ln == 0 or ln > 512:
            return None
        raw = data[namesOff+off+6:namesOff+off+6+ln*2]
        if len(raw) < ln*2:
            return None
        try:
            s = raw.decode('utf-16-be')
        except UnicodeDecodeError:
            return None
        return s if all(c.isprintable() for c in s) else None

    # parse nodes
    nodes = []
    pos = treeOff
    while pos + NODE <= len(data):
        nameOff, flags = struct.unpack('>IH', data[pos:pos+6])
        if flags & FLAG_DIR:
            childCount, childOff = struct.unpack('>II', data[pos+6:pos+14])
            nodes.append({'nameOff': nameOff, 'flags': flags, 'childCount': childCount, 'childOff': childOff})
        else:
            territory, language, blobOff = struct.unpack('>HHI', data[pos+6:pos+14])
            nodes.append({'nameOff': nameOff, 'flags': flags, 'blobOff': blobOff})
        pos += NODE
    print(f"nodes: {len(nodes)} ({sum(1 for n in nodes if n['flags'] & FLAG_DIR)} dirs)")

    # validate name resolution
    badnames = sum(1 for n in nodes if read_name_at(n['nameOff']) is None)
    print(f"nodes with unresolvable names: {badnames}")

    manifest = []
    def name_of(n):
        s = read_name_at(n['nameOff'])
        return s if s is not None else f"?off{n['nameOff']}"

    # root (node 0) has a synthetic empty name - skip it
    def walk(i, path):
        n = nodes[i]
        p = f"{path}/{name_of(n)}" if path else name_of(n)
        if n['flags'] & FLAG_DIR:
            for c in range(n['childCount']):
                ci = n['childOff'] + c
                if 0 <= ci < len(nodes):
                    walk(ci, p)
        else:
            manifest.append({'path': p, 'flags': n['flags'], 'blobOff': n['blobOff']})

    for c in range(nodes[0]['childCount']):
        walk(nodes[0]['childOff'] + c, '')
    print(f"files: {len(manifest)}")

    ok = bad = skip = 0
    for m in manifest:
        rel = m['path'].lstrip('/')
        dest = os.path.join(outdir, rel)
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        bo = dataOff + m['blobOff']
        if bo + 4 > len(data):
            bad += 1
            continue
        size = struct.unpack('>I', data[bo:bo+4])[0]
        blob = data[bo+4:bo+4+size]
        if len(blob) != size:
            bad += 1
            continue
        if m['flags'] & FLAG_ZSTD:
            try:
                import zstandard
                blob = zstandard.ZstdDecompressor().decompress(blob, max_output_size=100<<20)
            except ImportError:
                skip += 1
                with open(dest + '.zst', 'wb') as f:
                    f.write(blob)
                continue
        elif m['flags'] & FLAG_COMPRESSED:
            try:
                blob = zlib.decompress(blob)
            except Exception:
                bad += 1
                continue
        with open(dest, 'wb') as f:
            f.write(blob)
        ok += 1
    print(f"extracted ok={ok} bad={bad} zst-skip={skip}")
    json.dump(manifest, open(os.path.join(outdir, '_manifest.json'), 'w'), indent=0)

if __name__ == '__main__':
    main()
