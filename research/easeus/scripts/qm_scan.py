#!/usr/bin/env python3
"""Tolerant QM scanner: extracts all [tag][u32 BE len][bytes] records.
Classifies records by content heuristics: class-like names = contexts,
sentences = sources/targets. Also handles UTF-16LE targets (tag 3)."""
import struct, sys, json, re

def scan(path):
    data = open(path, 'rb').read()
    records = []
    pos = 16  # after magic
    end = len(data)
    while pos + 5 <= end:
        tag = data[pos]
        if tag == 1:  # Tag_End — skip
            pos += 1
            continue
        ln = struct.unpack('>I', data[pos+1:pos+5])[0]
        if ln > 0 and pos + 5 + ln <= end and ln < 100000:
            blob = data[pos+5:pos+5+ln]
            # plausible string?
            try:
                s = blob.decode('utf-8')
                ok = all(c.isprintable() or c in '\n\t\r\x01' for c in s)
            except UnicodeDecodeError:
                # maybe UTF-16
                try:
                    s = blob.decode('utf-16-le')
                    ok = all(c.isprintable() or c in '\n\t\r' for c in s)
                    tag = f'{tag}u16'
                except Exception:
                    ok = False
                    s = None
            if ok and s is not None and s.strip():
                records.append({'tag': str(tag), 'len': ln, 'text': s})
                pos += 5 + ln
                continue
        # not a record: step 1 byte (index sections etc.)
        pos += 1
    return records

if __name__ == '__main__':
    recs = scan(sys.argv[1])
    out = sys.argv[2]
    json.dump(recs, open(out, 'w', encoding='utf-8'), ensure_ascii=False, indent=0)
    # stats
    from collections import Counter
    tags = Counter(r['tag'] for r in recs)
    print(f"total records: {len(recs)}", file=sys.stderr)
    for t, n in tags.most_common():
        print(f"  tag {t}: {n}", file=sys.stderr)
    # sample by tag
    for t in tags:
        samples = [r['text'][:70] for r in recs if r['tag'] == t][:6]
        print(f"\n== tag {t} samples ==", file=sys.stderr)
        for s in samples:
            print(f"   {s!r}", file=sys.stderr)
