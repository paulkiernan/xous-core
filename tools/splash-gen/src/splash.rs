//! "ZEROCOOL" scene-NFO splash screen, rendered directly into the life
//! bitboard so every on-pixel of the image *is* the initial population.
//!
//! Aesthetic: 90s/early-2000s warez .NFO release card. A double box-drawing
//! border frames the screen; Bayer-4x4 dithered gradient rules separate the
//! release header (the big chunky ZEROCOOL glyphs with a bright-to-dim
//! gradient and drop shadow) from the "ESTABLISHED 1989" / "PRESENTS" lines
//! in the loader's 6x12 bitmap font, finished with a checker-dithered shade
//! strip. Everything on screen at boot becomes the Game of Life seed once
//! the splash period elapses.

use crate::MAX_WORDS;

const W: isize = 128;
const H: isize = 128;

/// Bayer 4x4 ordered dither matrix (0..15)
const BAYER4: [[u16; 4]; 4] = [
    [0, 8, 2, 10],
    [12, 4, 14, 6],
    [3, 11, 1, 9],
    [15, 7, 13, 5],
];

/// glyphs at 8x9; all are 7 columns of ink with column 7 reserved as the
/// inter-letter gap. Scaled x2 that is 14px of glyph + 2px gap = 16px per
/// character, exactly 128px for "ZEROCOOL".
const GLYPHS: [&[&str]; 8] = [
    &[
        "XXXXXXX.",
        "......X.",
        "......X.",
        ".....X..",
        "....X...",
        "...X....",
        "..X.....",
        ".X......",
        "XXXXXXX.",
    ],
    &[
        "XXXXXXX.",
        "X.......",
        "X.......",
        "XXXXXX..",
        "X.......",
        "X.......",
        "X.......",
        "X.......",
        "XXXXXXX.",
    ],
    &[
        "XXXXXXX.",
        "X.....X.",
        "X.....X.",
        "XXXXXX..",
        "X...X...",
        "X...X...",
        "X...X...",
        "X....X..",
        "X.....X.",
    ],
    &[
        "XXXXXXX.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "XXXXXXX.",
    ],
    &[
        "XXXXXXX.",
        "X.......",
        "X.......",
        "X.......",
        "X.......",
        "X.......",
        "X.......",
        "X.......",
        "XXXXXXX.",
    ],
    &[
        "XXXXXXX.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "XXXXXXX.",
    ],
    &[
        "XXXXXXX.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "X.....X.",
        "XXXXXXX.",
    ],
    &[
        "X.......",
        "X.......",
        "X.......",
        "X.......",
        "X.......",
        "X.......",
        "X.......",
        "X.......",
        "XXXXXXX.",
    ],
];

fn set_cell(field: &mut [u32; MAX_WORDS], x: isize, y: isize) {
    if x < 0 || x >= W || y < 0 || y >= H {
        return;
    }
    let bit = (y * W + x) as usize;
    field[bit / 32] |= 1 << (bit % 32);
}

/// Bayer-dither a pixel at `shade` (0..=255); true = draw it
fn dithered(shade: u8, x: isize, y: isize) -> bool {
    (shade as u16 * 16 / 255) > BAYER4[(y & 3) as usize][(x & 3) as usize]
}

/// draw a 2x2 dithered block (the glyphs are rendered at x2 scale)
fn set_block_dither(field: &mut [u32; MAX_WORDS], x: isize, y: isize, shade: u8) {
    for dy in 0..2 {
        for dx in 0..2 {
            if dithered(shade, x + dx, y + dy) {
                set_cell(field, x + dx, y + dy);
            }
        }
    }
}

/// a compact 5x7 bitmap font table for just the glyphs the captions need,
/// stored as data so the code footprint stays tiny (the app must stay well
/// under the swap image's 4096-byte block boundary)
const FONT5X7: [[u8; 7]; 32] = [
    // A, B, D, E, I, L, N, P
    [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
    [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
    [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
    [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
    [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111],
    [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
    [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
    [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
    // R, S, T, 0, 1, 8, 9, z
    [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
    [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
    [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
    [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
    [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
    [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
    [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110],
    [0b00000, 0b00000, 0b11111, 0b00010, 0b00100, 0b01000, 0b11111],
    // e, r, o, @, p, a, u, l
    [0b00000, 0b00000, 0b01110, 0b10001, 0b11111, 0b10000, 0b01110],
    [0b00000, 0b00000, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000],
    [0b00000, 0b00000, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110],
    [0b01110, 0b10001, 0b10111, 0b10101, 0b10111, 0b10001, 0b01110],
    [0b00000, 0b00000, 0b11110, 0b10001, 0b10001, 0b11110, 0b10000],
    [0b00000, 0b00000, 0b01110, 0b00001, 0b01111, 0b10001, 0b01111],
    [0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b10011, 0b01101],
    [0b00000, 0b00000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01110],
    // y, n, m, i, c, ., space, space
    [0b00000, 0b00000, 0b10001, 0b10001, 0b10011, 0b01101, 0b00001],
    [0b00000, 0b00000, 0b11110, 0b10001, 0b10001, 0b10001, 0b10001],
    [0b00000, 0b00000, 0b11011, 0b10101, 0b10101, 0b10101, 0b10101],
    [0b00000, 0b00000, 0b00100, 0b00000, 0b00100, 0b00100, 0b01110],
    [0b00000, 0b00000, 0b01110, 0b10000, 0b10000, 0b10000, 0b01110],
    [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100],
    [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
    [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000],
];

fn glyph_index(c: char) -> usize {
    match c {
        'A' => 0, 'B' => 1, 'D' => 2, 'E' => 3, 'H' => 30, 'I' => 4, 'L' => 5, 'N' => 6, 'P' => 7,
        'R' => 8, 'S' => 9, 'T' => 10, '0' => 11, '1' => 12, '8' => 13, '9' => 14, 'z' => 15,
        'e' => 16, 'r' => 17, 'o' => 18, '@' => 19, 'p' => 20, 'a' => 21, 'u' => 22, 'l' => 23,
        'y' => 24, 'n' => 25, 'm' => 26, 'i' => 27, 'c' => 28, '.' => 29, ' ' => 31, _ => 31,
    }
}

/// render a line of 5x7 font text at (x0, y0), `pitch` px per char
fn render_text5(field: &mut [u32; MAX_WORDS], text: &str, x0: isize, y0: isize, pitch: isize, shade: u8) {
    for (i, ch) in text.chars().enumerate() {
        let g = FONT5X7[glyph_index(ch)];
        for row in 0..7usize {
            let bits = g[row];
            for col in 0..5usize {
                if bits & (1 << (4 - col)) != 0 {
                    let px = x0 + (i as isize) * pitch + col as isize;
                    let py = y0 + row as isize;
                    if dithered(shade, px, py) {
                        set_cell(field, px, py);
                    }
                }
            }
        }
    }
}

/// horizontal rule: 2px tall, Bayer-dithered left-to-right gradient,
/// spanning the inner border
fn divider(field: &mut [u32; MAX_WORDS], y: isize) {
    for x in 2..(W - 2) {
        let shade = 255u8 - ((x * 150) / (W - 5)) as u8;
        if dithered(shade, x, y) {
            set_cell(field, x, y);
        }
        if dithered(shade, x, y + 1) {
            set_cell(field, x, y + 1);
        }
    }
}

/// 50% checker-dither shade strip, the classic NFO "shaded" filler line
fn checker_strip(field: &mut [u32; MAX_WORDS], y: isize) {
    for x in 2..(W - 2) {
        if (x + y) & 1 == 0 {
            set_cell(field, x, y);
        }
    }
}

/// Render the full splash card into the bitboard.
pub fn render(field: &mut [u32; MAX_WORDS], _w: isize, _h: isize) {
    // double box-drawing border (the "+" corner blocks fall out of the two
    // nested frames)
    for x in 0..W {
        set_cell(field, x, 0);
        set_cell(field, x, H - 1);
    }
    for y in 0..H {
        set_cell(field, 0, y);
        set_cell(field, W - 1, y);
    }
    for x in 1..(W - 1) {
        set_cell(field, x, 1);
        set_cell(field, x, H - 2);
    }
    for y in 1..(H - 1) {
        set_cell(field, 1, y);
        set_cell(field, W - 2, y);
    }

    // release header: rule, title, rule
    divider(field, 8);

    for (g, glyph) in GLYPHS.iter().enumerate() {
        let x0 = (g as isize) * 16;
        for (row, line) in glyph.iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                if ch != 'X' {
                    continue;
                }
                let sx = x0 + (col as isize) * 2;
                let sy = 14 + (row as isize) * 2;
                set_block_dither(field, sx + 2, sy + 2, 50); // shadow
                let shade = 255u8 - ((row * 150) / 8) as u8; // 255 -> 105
                set_block_dither(field, sx, sy, shade);
            }
        }
    }

    divider(field, 36);

    // info lines: since when, how to reach us, and who presents.
    // Solid ink (255): Bayer dithering at lower shades chips 5x7 strokes
    // (the A's crossbar, the 9's tail). Dithering stays on the title
    // gradient, the rules, and the checker strip where it belongs.
    render_text5(field, "ESTABLISHED 1989", 16, 48, 6, 255);
    render_text5(field, "zero@paulynomial.com", 4, 64, 6, 255);
    render_text5(field, "PRESENTS", 40, 106, 6, 255);

    // footer rule + shaded strip
    divider(field, 98);
    checker_strip(field, 118);
}
