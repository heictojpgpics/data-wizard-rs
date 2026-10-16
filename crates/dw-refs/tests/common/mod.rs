//! Synthetic ReFS volume builder (test-only): mirrors the layouts
//! empirically validated against the real CI-generated ReFS 3.14 volume.

#![allow(dead_code)]

use dw_refs::record::FILEINFO_LEN;

pub const VOL: usize = 8 * 1024 * 1024;

fn mk_ft(secs: i64) -> u64 {
    ((secs + 11_644_473_600) * 10_000_000) as u64
}

/// One name record: id@+0x18, a@+0x28, b@+0x2a, name@+0x2c.
fn name_record(id: u64, name: &str) -> Vec<u8> {
    // Name data starts at +0x2c; size the record to fit the name.
    let chars = name.encode_utf16().count();
    let mut r = vec![0u8; 0x2c + chars * 2 + 2];
    r[0x18..0x20].copy_from_slice(&id.to_le_bytes());
    let chars16 = chars as u16;
    r[0x28..0x2a].copy_from_slice(&(chars16 + 2).to_le_bytes());
    r[0x2a..0x2c].copy_from_slice(&(chars as u16 * 2).to_le_bytes());
    let mut off = 0x2c;
    for cu in name.encode_utf16() {
        r[off..off + 2].copy_from_slice(&cu.to_le_bytes());
        off += 2;
    }
    r
}

/// One file-info record (0xa8 bytes).
fn fileinfo_record(id: u64, size: u64, cr: i64, mt: i64, ct: i64, at: i64) -> Vec<u8> {
    let mut r = vec![0u8; FILEINFO_LEN as usize];
    r[0..4].copy_from_slice(&FILEINFO_LEN.to_le_bytes());
    r[0x28..0x30].copy_from_slice(&mk_ft(cr).to_le_bytes());
    r[0x30..0x38].copy_from_slice(&mk_ft(mt).to_le_bytes());
    r[0x38..0x40].copy_from_slice(&mk_ft(ct).to_le_bytes());
    r[0x40..0x48].copy_from_slice(&mk_ft(at).to_le_bytes());
    r[0x58..0x60].copy_from_slice(&size.to_le_bytes());
    r[0x80..0x88].copy_from_slice(&id.to_le_bytes());
    r
}

fn msb_page(payload: &[u8]) -> Vec<u8> {
    let mut p = vec![0u8; 4096];
    p[0..4].copy_from_slice(b"MSB+");
    p[4..8].copy_from_slice(&2u32.to_le_bytes());
    p[0x0c..0x10].copy_from_slice(&0xc9f9af4cu32.to_le_bytes());
    for i in 0..4u64 {
        p[0x20 + (i * 8) as usize..0x28 + (i * 8) as usize]
            .copy_from_slice(&(0x24 + i).to_le_bytes());
    }
    let n = payload.len().min(4096 - 0x80);
    p[0x80..0x80 + n].copy_from_slice(&payload[..n]);
    p
}

pub fn build_volume() -> Vec<u8> {
    let mut img = vec![0u8; VOL];
    // boot sector
    img[0x03..0x07].copy_from_slice(b"ReFS");
    img[0x10..0x14].copy_from_slice(b"FSRS");
    img[0x14..0x16].copy_from_slice(&0xd5cbu16.to_le_bytes());
    img[0x18..0x20].copy_from_slice(&((VOL / 512) as u64 - 2048).to_le_bytes());
    img[0x20..0x24].copy_from_slice(&512u32.to_le_bytes());
    img[0x24..0x28].copy_from_slice(&8u32.to_le_bytes());
    img[0x28..0x2a].copy_from_slice(&3u16.to_le_bytes());
    img[0x2a..0x2c].copy_from_slice(&14u16.to_le_bytes());
    img[0x38..0x40].copy_from_slice(&0x2c1877f51877bc80u64.to_le_bytes());

    // page 1: names for hello.txt (id 2), inner.bin (id 3)
    let mut payload = Vec::new();
    payload.extend_from_slice(&name_record(2, "hello.txt"));
    payload.extend_from_slice(&name_record(3, "inner.bin"));
    let p1 = msb_page(&payload);
    img[0x24000..0x24000 + 4096].copy_from_slice(&p1);

    // page 2: file info for both
    let mut payload2 = Vec::new();
    payload2.extend_from_slice(&fileinfo_record(
        2,
        12,
        1_600_000_000,
        1_700_000_000,
        1_650_000_000,
        1_600_000_000,
    ));
    payload2.extend_from_slice(&fileinfo_record(
        3,
        4096,
        1_500_000_000,
        1_600_000_000,
        1_550_000_000,
        1_500_000_000,
    ));
    let p2 = msb_page(&payload2);
    img[0x28000..0x28000 + 4096].copy_from_slice(&p2);

    // headless old-generation page: gone.live (id 9) was deleted before
    // the final state; its old page copy survives without a header.
    let mut payload3 = Vec::new();
    payload3.extend_from_slice(&name_record(9, "gone.live"));
    payload3.extend_from_slice(&fileinfo_record(
        9,
        777,
        1_400_000_000,
        1_450_000_000,
        1_420_000_000,
        1_400_000_000,
    ));
    img[0x30000..0x30000 + payload3.len()].copy_from_slice(&payload3);

    img
}
