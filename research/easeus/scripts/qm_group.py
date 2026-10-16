#!/usr/bin/env python3
"""Group QM records into context→[strings] map, preserving file order."""
import json, sys
from collections import OrderedDict

recs = json.load(open(sys.argv[1], encoding='utf-8'))
# Re-walk the record stream: consecutive [6 src][7 ctx] = one message.
# Order observed in file: (7 ctx can precede or follow its 6 src within a record block;
# each message block ends with tag 1.) We reconstruct by pairing each tag-6 with the
# NEAREST tag-7 (same message block).
messages = []
pending_src = None
pending_ctx = None
def flush():
    global pending_src, pending_ctx
    if pending_src is not None or pending_ctx is not None:
        messages.append({'context': pending_ctx, 'source': pending_src})
        pending_src, pending_ctx = None, None

# The raw scanner skipped index bytes; safer: re-parse in ORDER with a strict walker
# that tolerates single-byte skips but keeps sequence.
import struct
data = open(sys.argv[3] if len(sys.argv) > 3 else '/dev/null', 'rb').read() if len(sys.argv) > 3 else None

# simpler: use record order from the JSON (scanner preserved order)
cur_ctx = None
for r in recs:
    if r['tag'] == '7':
        cur_ctx = r['text']
        if pending_src is not None:
            messages.append({'context': cur_ctx, 'source': pending_src})
            pending_src = None
    elif r['tag'] == '6':
        if pending_src is not None:  # two 6s in a row — previous had no ctx yet
            messages.append({'context': cur_ctx, 'source': pending_src})
        pending_src = r['text']
flush()

# dedupe
seen = set()
uniq = []
for m in messages:
    key = (m['context'], m['source'])
    if key in seen:
        continue
    seen.add(key)
    uniq.append(m)

groups = OrderedDict()
for m in uniq:
    groups.setdefault(m['context'] or '(no ctx)', []).append(m['source'])

out = sys.argv[2]
json.dump(groups, open(out, 'w', encoding='utf-8'), ensure_ascii=False, indent=1)
print(f"messages: {len(uniq)}, contexts: {len(groups)}", file=sys.stderr)
for c, arr in sorted(groups.items(), key=lambda x: -len(x[1])):
    print(f"  {len(arr):4d}  {c}", file=sys.stderr)
