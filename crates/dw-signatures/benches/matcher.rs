//! Matcher throughput: how fast can one pass over a buffer reject or
//! confirm 258 signature magics. Reports MB/s for the bucketed scan.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use dw_signatures::{Matcher, SignatureDb};

/// Pseudo-random fill (xorshift): the realistic mixed-data case.
fn random_buffer(mib: usize) -> Vec<u8> {
    let mut state = 0x2545F4914F6CDD1Du64;
    (0..mib * 1024 * 1024)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 33) as u8
        })
        .collect()
}

/// All-zero fill: the pathological-but-real case (sparse/wiped regions;
/// 19 embedded signatures start with 0x00 so every position verifies).
fn zero_buffer(mib: usize) -> Vec<u8> {
    vec![0u8; mib * 1024 * 1024]
}

/// Random data with JPEG magics seeded every 1 MiB.
fn seeded_buffer(mib: usize) -> Vec<u8> {
    let mut buf = random_buffer(mib);
    for chunk in buf.chunks_mut(1024 * 1024) {
        chunk[0..4].copy_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0]);
    }
    buf
}

fn bench_matcher(c: &mut Criterion) {
    let db = SignatureDb::from_embedded();
    let matcher = Matcher::new(&db);

    for mib in [4usize, 16] {
        for (name, buf) in [
            ("random", random_buffer(mib)),
            ("zero_filled", zero_buffer(mib)),
            ("with_jpeg_hits", seeded_buffer(mib)),
        ] {
            let mut group = c.benchmark_group(format!("bucketed_matcher_{name}"));
            group.throughput(criterion::Throughput::Bytes(buf.len() as u64));
            group.bench_with_input(BenchmarkId::new("scan", mib), &buf, |b, buf| {
                b.iter(|| matcher.find_headers(buf, 0))
            });
            group.finish();
        }
    }
}

criterion_group!(benches, bench_matcher);
criterion_main!(benches);
