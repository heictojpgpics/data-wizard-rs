//! End-to-end carve throughput: signature scan + tiered validation over a
//! synthetic volume. Reports MB/s of region scanned.

use std::sync::atomic::AtomicBool;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use dw_carver::header_footer::carve_region;
use dw_core::ResultsStore;
use dw_disk::{DiskSource, FileBackedSource};
use dw_signatures::SignatureDb;

/// A minimal structurally valid JPEG (analysers must reach EOI).
fn minimal_jpeg(payload: usize) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8];
    v.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
    v.extend_from_slice(&[0x00; 14]);
    v.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x45]);
    v.extend_from_slice(&[0x11; 67]);
    v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11]);
    v.extend_from_slice(&[0x22; 15]);
    v.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x0C]);
    v.extend_from_slice(&[0x33; 10]);
    v.extend_from_slice(&vec![0x55u8; payload]);
    v.extend_from_slice(&[0xFF, 0xD9]);
    v
}

fn synthetic_volume(mib: usize) -> Vec<u8> {
    let mut buf = vec![0x37u8; mib * 1024 * 1024];
    // One small JPEG every 256 KiB: ~64 files per 16 MiB.
    let jpg = minimal_jpeg(512);
    let mut off = 1024usize;
    while off + jpg.len() < buf.len() {
        buf[off..off + jpg.len()].copy_from_slice(&jpg);
        off += 256 * 1024;
    }
    buf
}

fn bench_carve(c: &mut Criterion) {
    let db = SignatureDb::from_embedded();

    for mib in [4usize, 16] {
        let volume = synthetic_volume(mib);
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), &volume).unwrap();
        let source = FileBackedSource::open(temp.path()).unwrap();
        let read_at = source.as_read_at();

        let mut group = c.benchmark_group("carve_region");
        group.throughput(criterion::Throughput::Bytes(volume.len() as u64));
        group.sample_size(10);
        group.bench_with_input(BenchmarkId::new("parallel", mib), &read_at, |b, ra| {
            b.iter(|| {
                let results = ResultsStore::new(1_000_000, None);
                carve_region(
                    ra.as_ref(),
                    0,
                    volume.len() as u64,
                    &db,
                    &results,
                    Some(&AtomicBool::new(false)),
                    None,
                )
            })
        });
        group.finish();
    }
}

criterion_group!(benches, bench_carve);
criterion_main!(benches);
