# Techniques extracted from reference repositories (session 5)

Studied: tobi/disktree, Byron/dua-cli, IgorMundstein/WinMemoryCleaner, vyrti/cleaner
(method: two parallel deep-read agents; excerpts verified against source)

## Scanning architecture (disktree + dua-cli)

- dua-lib: hand-rolled work-stealing pool on crossbeam (Injector + per-worker
  LIFO deque + Parker). Injector-first job search so new roots never starve
  behind a big tree. A successful steal wakes exactly one more idle worker.
- dua-lib: entry chunking (ReadDir -> Stat jobs of 4) with inline fallback when
  the local queue holds >= 64 stat jobs — bounds queue memory on wide dirs.
- disktree: rayon::scope with flat tasks (worker stack never grows with tree
  depth); per-directory `Arc<PendingDir>` with pending: AtomicUsize reaching
  zero -> bottom-up assembly. Dedicated static walk pool capped at 16 threads
  on Windows (measured: 3.4M files in 6.5s on 16T vs 8.1s on 24T).
- disktree: admission controller — binary search over worker count retaining
  >= 80% throughput, plus system-CPU governor (0.80 ceiling via sysinfo).
- Memory bounding: dua output channel is sync_channel(threads x 2 batches);
  workers block when the consumer is slow. Never unbounded.

## Progress reporting

- disktree: plain AtomicU64 counters snapshot by the UI (Relaxed ordering —
  "a progress meter, not a synchronisation point"); TALLY_EVERY = 1024 local
  tallies flushed per directory to avoid cache-line bouncing; error ring capped
  at 50 verbatim strings + total count; UI polls every 110ms with epoch
  counters invalidating stale pollers on re-scan.
- dua-cli: Throttle thread flips an AtomicBool every 100ms — zero clock reads
  in the scan loop; entries/sec computed client-side.

## Cancellation (three layers, dua-lib)

1. Cooperative AtomicBool polled per job
2. recv_timeout(100ms) so a stuck channel cannot wedge cancellation
3. Channel-disconnect abort for blocked senders + CancelOnDrop RAII guard

## Data structures

- dua: arena tree (nodes: Vec<TreeNode>, names: Vec<u8>) — 64 bytes/node,
  u32 indices with NonZeroU32 ids, no HashMap<PathBuf, _>; dense DirectoryId
  side table maps walker ids to tree nodes without path comparisons.
- disktree: 8 MiB lock-free atomic bitmap for inode dedup of small ids +
  64 sharded Mutex<FxHashSet> for outliers (4.1M ids: 150ms sharded vs 1.2s
  single lock).
- Snapshots: ULEB128 varints + flags byte, atomic persist via tempfile.

## IO layer (disktree mft.rs — the model for our raw reads)

- FILE_FLAG_NO_BUFFERING past the file cache (data read once, not copied
  twice); 16 MiB chunks with aligned buffers, positional seek_read_exact;
  tolerates free gaps up to 1 MiB (a request costs ~0.29ms — as much as
  reading 1.3 MiB more); rayon chunk parsing; total graceful fallback to
  the normal walk on any parse doubt.
- dua macOS: CPU-time vs wall-time probe over two bulk refills decides
  serial-bulk vs parallel-small-reads, re-probed per filesystem.

## UX patterns (WinMemoryCleaner + cleaner)

- Phase machine: enum Probing -> Ready -> Confirm/Typing -> Running ->
  Done(summary); the phase gates which controls are active.
- Confirmation in place with concrete quantified preview ("Recover 12
  files, 3.4 GB?"), expiring when focus/selection moves.
- Tiered risk: Safe (pre-checked) / opt-in / destructive (type the word).
- TOCTOU re-validation immediately before any mutation; "path type changed
  since scan" rejection.
- Allowlist roots for outputs (reject root-equal, reject `..`).
- Streaming results: bounded(1024) channel, batch UI updates (64-1024).
- Selection restoration across refreshes (restore_path/restore_name).
- Honest loss-oriented copy: every row answers "what do I lose?" —
  "the previous Windows install; removes your ability to roll back".
  No marketing voice in-app; sizes always humanized.

## Adopted for data-wizard (this session)

1. Tally-batched relaxed-atomic progress in dw-core ScanProgress
2. Bounded handoff in carver (never unbounded queues)
3. Three-layer cancellation semantics on scan handles
4. Multi-pattern pre-filter scan over aligned chunks (dw-signatures)
5. Phase-machine UI state + quantified confirmations + expiring intents
6. Honest copy style: quantify, name the risk, no filler words
