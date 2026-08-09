// Conway's Game of Life for the DC34 badge (baosec target).
//
// On baosec there is no gam service: apps draw straight to the badge's display
// through the Gfx API served by bao-video (which owns the camera + OLED). This
// app takes over the entire 128x128 framebuffer and renders a toroidal life
// field at ~4.5 generations/second. Each frame clears to black, batches the
// live cells as 1px filled rectangles in white (flushing the list when it nears
// a page), then flushes the framebuffer to the display.
//
// Seeded with a Gosper glider gun, gliders, an R-pentomino and a few still
// lifes; the toroidal wrap makes patterns circulate forever.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

use ux_api::minigfx::*;
use ux_api::service::gfx::Gfx;

const MAX_W: isize = 128;
const MAX_H: isize = 128;
const MAX_WORDS: usize = (MAX_W as usize) * (MAX_H as usize) / 32;
const TICK_MS: usize = 220; // ~4.5 generations per second

struct Gol {
    gfx: Gfx,
    w: isize,
    h: isize,
    /// current generation bitboard; bit = 1 => live cell, row-major, LSB-first
    cur: [u32; MAX_WORDS],
    next: [u32; MAX_WORDS],
}

impl Gol {
    fn new(gfx: Gfx, screensize: Point) -> Self {
        assert_eq!(screensize.x, MAX_W, "expected a full-width display");
        let mut gol = Gol { gfx, w: screensize.x, h: screensize.y, cur: [0; MAX_WORDS], next: [0; MAX_WORDS] };
        gol.seed();
        gol
    }

    fn set(&mut self, x: isize, y: isize, alive: bool) {
        if x < 0 || x >= self.w || y < 0 || y >= self.h {
            return;
        }
        let bit = (y * self.w + x) as usize;
        if alive {
            self.cur[bit / 32] |= 1 << (bit % 32);
        } else {
            self.cur[bit / 32] &= !(1 << (bit % 32));
        }
    }

    fn is_alive(&self, x: isize, y: isize) -> bool {
        let bit = (y * self.w + x) as usize;
        (self.cur[bit / 32] >> (bit % 32)) & 1 == 1
    }

    /// neighbour count with toroidal wrap
    fn neighbours(&self, x: isize, y: isize) -> u8 {
        let mut n = 0u8;
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (x + dx).rem_euclid(self.w);
                let ny = (y + dy).rem_euclid(self.h);
                if self.is_alive(nx, ny) {
                    n += 1;
                }
            }
        }
        n
    }

    fn step(&mut self) {
        self.next = [0; MAX_WORDS];
        for y in 0..self.h {
            for x in 0..self.w {
                let n = self.neighbours(x, y);
                let alive = if self.is_alive(x, y) { n == 2 || n == 3 } else { n == 3 };
                if alive {
                    let bit = (y * self.w + x) as usize;
                    self.next[bit / 32] |= 1 << (bit % 32);
                }
            }
        }
        core::mem::swap(&mut self.cur, &mut self.next);
    }

    /// clear to black, draw the live cells, then push the framebuffer out
    fn draw(&self) {
        let clear = ClipObjectType::Rect(Rectangle::new_with_style(
            Point::new(0, 0),
            Point::new(self.w - 1, self.h - 1),
            DrawStyle { fill_color: Some(PixelColor::Dark), stroke_color: None, stroke_width: 0 },
        ));
        let mut list = ObjectList::new();
        let _ = list.push(clear);

        // keep batches small enough that the rkyv-serialized message stays well
        // under the 4096-byte IPC page (a full ~64-item list overflows it and
        // the graphics server gets killed by the kernel)
        const MAX_PER_LIST: usize = 24;
        let mut n: usize = 0;
        for y in 0..self.h {
            for x in 0..self.w {
                if self.is_alive(x, y) {
                    let cell = ClipObjectType::Rect(Rectangle::new_with_style(
                        Point::new(x, y),
                        Point::new(x, y),
                        DrawStyle { fill_color: Some(PixelColor::Light), stroke_color: None, stroke_width: 0 },
                    ));
                    if n >= MAX_PER_LIST {
                        self.gfx.draw_object_list(list).expect("couldn't draw cells");
                        list = ObjectList::new();
                        n = 0;
                    }
                    let _ = list.push(cell);
                    n += 1;
                }
            }
        }
        if !list.list.is_empty() {
            self.gfx.draw_object_list(list).expect("couldn't draw cells");
        }
        self.gfx.flush().expect("couldn't flush display");
    }

    /// seed a Gosper glider gun, gliders, an R-pentomino and a few still lifes
    fn seed(&mut self) {
        self.put_pattern(GOSPER_GUN, 5, 5);
        self.put_pattern(GLIDER_SE, 40, 30);
        self.put_pattern(GLIDER_SE, 60, 45);
        self.put_pattern(GLIDER_NE, 80, 20);
        self.put_pattern(R_PENTOMINO, 100, 60);
        self.put_pattern(BLOCK, 5, 105);
        self.put_pattern(BEEHIVE, 30, 110);
        self.put_pattern(LOAF, 70, 112);
    }

    /// blit a pattern's rows of '*' (alive) / '.' (dead) starting at (ox, oy)
    fn put_pattern(&mut self, pattern: &[&str], ox: isize, oy: isize) {
        for (row, line) in pattern.iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                if ch == '*' {
                    self.set(ox + col as isize, oy + row as isize, true);
                }
            }
        }
    }
}

// Gosper glider gun -- fires a glider down its row every 30 generations
const GOSPER_GUN: &[&str] = &[
    "........................*...........",
    "......................*.*...........",
    "............**......**............**",
    "...........*...*....**............**",
    "**........*.....*...**..............",
    "**........*...*.**....*.*...........",
    "..........*.....*.......*...........",
    "...........*...*....................",
    "............**......................",
];

const GLIDER_SE: &[&str] = &[".*.", "..*", "***"];

const GLIDER_NE: &[&str] = &[".*.", "*..", "***"];

const R_PENTOMINO: &[&str] = &[".**", "**.", ".*."];

const BLOCK: &[&str] = &["**", "**"];

const BEEHIVE: &[&str] = &[".**.", "*..*", ".**."];

const LOAF: &[&str] = &[".**.", "*..*", ".*.*", "..*."];

fn main() -> ! {
    log_server::init_wait().unwrap();
    log::set_max_level(log::LevelFilter::Info);
    log::info!("Game of Life PID is {}", xous::process::id());

    let xns = xous_names::XousNames::new().unwrap();
    let gfx = Gfx::new(&xns).expect("can't connect to GFX");
    let screensize = gfx.screen_size().expect("couldn't get screen size");
    log::info!("game of life screen: {}x{}", screensize.x, screensize.y);

    let gol = Gol::new(gfx, screensize);
    gol.draw(); // first paint

    let tt = ticktimer_server::Ticktimer::new().unwrap();
    let mut gol = gol;
    loop {
        tt.sleep_ms(TICK_MS).unwrap();
        gol.step();
        gol.draw();
    }
}
