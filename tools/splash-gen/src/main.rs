//! Generates the badge splash assets from the render code in `splash.rs`:
//!
//! - `splash.png` — 128x128 8-bit grayscale, Bayer dithering baked in. This is
//!   the review artifact: what you see is exactly what the badge displays.
//! - `splash_img.rs` — the same pixels as a `[u32; 512]` bitboard that the
//!   gameoflife app embeds and copies into its field at boot (every set bit
//!   becomes a Game of Life cell).
//!
//! Usage: `cargo run --release` from this directory. Rewrites the two files
//! in the app directory in place.

const MAX_WORDS: usize = 128 * 128 / 32;

mod splash;

use std::path::Path;

fn main() {
    let mut field = [0u32; MAX_WORDS];
    splash::render(&mut field, 128, 128);
    let on: u32 = field.iter().map(|w| w.count_ones()).sum();
    println!("splash rendered: {} on-pixels", on);

    let app_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps-baosec/gameoflife");
    let png_path = app_dir.join("splash.png");
    let rs_path = app_dir.join("src/splash_img.rs");
    write_png(&png_path, &field);
    write_rs(&rs_path, &field);
    println!("wrote {}\nwrote {}", png_path.display(), rs_path.display());
}

/// 128x128 bitboard -> 8-bit grayscale PNG. Uses a single uncompressed
/// deflate stored block so the writer needs no dependencies.
fn write_png(path: &Path, field: &[u32; MAX_WORDS]) {
    // raw scanlines: 1 filter byte (0 = None) + 128 gray bytes per row
    let mut raw = Vec::with_capacity(128 * 129);
    for y in 0..128usize {
        raw.push(0);
        for x in 0..128usize {
            let idx = y * 128 + x;
            let on = field[idx / 32] & (1 << (idx % 32)) != 0;
            raw.push(if on { 0xFF } else { 0x00 });
        }
    }
    // zlib: 0x78 0x01 header, one deflate stored block, adler32
    let mut z = vec![0x78, 0x01];
    let len = raw.len() as u32;
    z.push(1); // BFINAL=1, BTYPE=00
    z.extend_from_slice(&(len as u16).to_le_bytes());
    z.extend_from_slice(&(!(len as u16)).to_le_bytes());
    z.extend_from_slice(&raw);
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let ihdr = {
        let mut d = Vec::new();
        d.extend_from_slice(&128u32.to_be_bytes());
        d.extend_from_slice(&128u32.to_be_bytes());
        d.extend_from_slice(&[8, 0, 0, 0, 0]); // bit depth 8, color type 0 (gray)
        d
    };
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &z);
    chunk(&mut png, b"IEND", &[]);
    std::fs::write(path, png).expect("write splash.png");
}

/// 128x128 bitboard -> Rust source: `pub const SPLASH_IMG: [u32; 512]`,
/// MSB of each word = leftmost pixel of that row's 32-cell span.
fn write_rs(path: &Path, field: &[u32; MAX_WORDS]) {
    let mut out = String::new();
    out.push_str(
        "//! Pre-rendered ZEROCOOL splash card for the gameoflife badge app:\n\
         //! 128x128, 1 bit per pixel packed 32 pixels per u32 in row-major\n\
         //! order, bit 0 (LSB) of each word = leftmost pixel of that 32-cell\n\
         //! span -- the same layout the app's bitboard uses. Every set bit is\n\
         //! a live cell, so this array IS the Game of Life initial population\n\
         //! shown at boot.\n\
         //!\n\
         //! GENERATED FILE - do not edit by hand. To change the splash, edit\n\
         //! tools/splash-gen/src/splash.rs and run:\n\
         //!   cd tools/splash-gen && cargo run --release\n\
         //! which rewrites this file and splash.png (the review artifact).\n\n\
         pub const SPLASH_IMG: [u32; 512] = [\n",
    );
    for (i, w) in field.iter().enumerate() {
        if i % 4 == 0 {
            out.push_str("    ");
        }
        out.push_str(&format!("0x{:08x}, ", w));
        if i % 4 == 3 {
            out.push('\n');
        }
    }
    out.push_str("];\n");
    std::fs::write(path, out).expect("write splash_img.rs");
}

fn chunk(png: &mut Vec<u8>, typ: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    png.extend_from_slice(typ);
    png.extend_from_slice(data);
    let mut crc = Crc32::new();
    crc.update(typ);
    crc.update(data);
    png.extend_from_slice(&crc.finish().to_be_bytes());
}

struct Crc32 {
    table: [u32; 256],
    state: u32,
}

impl Crc32 {
    fn new() -> Self {
        let mut table = [0u32; 256];
        for (n, slot) in table.iter_mut().enumerate() {
            let mut c = n as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
            }
            *slot = c;
        }
        Crc32 { table, state: 0xFFFF_FFFF }
    }
    fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.state = self.table[((self.state ^ b as u32) & 0xFF) as usize] ^ (self.state >> 8);
        }
    }
    fn finish(&self) -> u32 {
        self.state ^ 0xFFFF_FFFF
    }
}

fn adler32(data: &[u8]) -> u32 {
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}
