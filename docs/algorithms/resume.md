# Scan resume (checkpoints)

> Status: implemented (`dw-core/src/checkpoint.rs`, app commands
> save_scan / list_saved_scans / resume_scan / delete_saved_scan).

## Checkpoint content

- scan id, config, lifecycle state, created_at
- source identity: URI + size + BLAKE3 fingerprint
- FULL results snapshot (entries with extents/timestamps, so recovery
  works after resume without rescanning)

## Source fingerprint (5-point sampling)

Windows of 1 MiB at 0%, 25%, 50%, 75% and the final MiB; sources <= 8 MiB
are hashed whole. BLAKE3-256. Purpose: detect a changed/swap image.

## Resume policy (strict)

- size differs -> refuse;
- fingerprint differs -> refuse ("entries would be attributed to the
  wrong data"). We NEVER resume against a different image.

## Resume mechanics

`ScanHandle::restore` re-creates the handle with the SAME id, config and
state, pre-seeding the results store. The resumed scan re-runs the plugin
pass (current plugin API is one call covering quick+deep); identical
findings appear twice (restored + re-walked) and are collapsed by
`ResultsStore::dedup_by_identity()` - latest wins (the newest pass has
the most complete extents/timestamps). Future API work: split
quick/deep so a resume can skip the quick phase.

## Serialization notes (postcard)

Checkpoints use postcard binary frames, written atomically
(temp file + fsync + rename), version-gated on load.
IMPORTANT: types that cross this boundary must NOT use
`skip_serializing_if` - postcard is positional; a skipped field
desynchronizes every following field. (RecoveredEntry carried this bug;
the checkpoint roundtrip test caught it.)

## Unknown-filesystem fallback

`Scan::run` returns `ScanOutcome { plugin_id, entries }`. When no plugin
claims the source, the APP layer (never dw-core: carver depends on core)
runs the raw-carving fallback for deep scans - the role the reference
product assigns to RAWSearchDLL.
