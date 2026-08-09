// Conway's Game of Life for the DC34 badge (baosec-lite target).
//
// On baosec there is no gam service: apps draw straight to the badge's display
// through the Gfx API served by bao-video (which owns the camera + OLED). This
// app takes over the entire 128x128 framebuffer and renders a toroidal life
// field at ~4.5 generations/second. Each frame clears to black, batches the
// live cells as 1px filled rectangles in white (flushing the list when it
// nears a page), then flushes the framebuffer to the display.
//
// Power management mirrors the stock badge firmware: the LIS2DH12 accelerometer
// motion interrupt is routed to this app; when the badge hasn't moved for
// IDLE_SECS the app stops pumping (so the kernel parks the CPU in WFI), turns
// the OLED off, and sleeps until motion wakes it back up. On the hosted
// emulator there is no accelerometer, so power management is disabled and the
// app runs continuously.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

use num_traits::*;
use ux_api::minigfx::*;
use ux_api::service::gfx::Gfx;
use xous::Message;

#[cfg(not(feature = "hosted-baosec"))]
mod power;

const MAX_W: isize = 128;
const MAX_H: isize = 128;
const MAX_WORDS: usize = (MAX_W as usize) * (MAX_H as usize) / 32;
const TICK_MS: usize = 220; // ~4.5 generations per second
/// seconds of no motion before the badge idles (screen off, CPU parked in WFI)
const IDLE_SECS: u64 = 60;

pub(crate) const GOL_SERVER_NAME: &str = "_Game of Life_";

#[derive(Debug, num_derive::FromPrimitive, num_derive::ToPrimitive)]
enum GolOp {
    /// advance one generation and redraw
    Pump = 0,
    /// accelerometer motion interrupt fired (wake source)
    Motion = 1,
    /// exit the application
    Quit = 2,
}

/// control messages for the pump thread
#[derive(Debug, num_derive::FromPrimitive, num_derive::ToPrimitive)]
enum PumpOp {
    Run,
    Stop,
    Pump,
    Quit,
}

struct Gol {
    gfx: Gfx,
    w: isize,
    h: isize,
    /// current generation bitboard; bit = 1 => live cell, row-major, LSB-first
    cur: [u32; MAX_WORDS],
    next: [u32; MAX_WORDS],
    tt: ticktimer_server::Ticktimer,
    /// timestamp (ms) of the last accelerometer motion event
    last_motion_ms: u64,
    /// true once we've gone to sleep (pump stopped, screen off)
    idle: bool,
    /// connection to the pump thread's control server
    pump_cid: Option<xous::CID>,
    /// accelerometer motion wake (badge only)
    #[cfg(not(feature = "hosted-baosec"))]
    power: Option<power::Power>,
}

impl Gol {
    fn new(gfx: Gfx, screensize: Point) -> Self {
        assert_eq!(screensize.x, MAX_W, "expected a full-width display");
        let tt = ticktimer_server::Ticktimer::new().unwrap();
        let now = tt.elapsed_ms();
        let mut gol = Gol {
            gfx,
            w: screensize.x,
            h: screensize.y,
            cur: [0; MAX_WORDS],
            next: [0; MAX_WORDS],
            tt,
            last_motion_ms: now,
            idle: false,
            pump_cid: None,
            #[cfg(not(feature = "hosted-baosec"))]
            power: None,
        };
        gol.seed();
        gol
    }

    /// true when the accelerometer is present and the motion IRQ is armed
    fn power_active(&self) -> bool {
        #[cfg(not(feature = "hosted-baosec"))]
        {
            self.power.is_some()
        }
        #[cfg(feature = "hosted-baosec")]
        {
            false
        }
    }

    /// initialize the accelerometer motion wake (badge only; no-op on the emu)
    fn arm_power(&mut self, sid: xous::SID) {
        #[cfg(not(feature = "hosted-baosec"))]
        {
            self.power = power::Power::new(sid, GolOp::Motion.to_usize().unwrap());
            if self.power.is_some() {
                log::info!("power management armed");
            }
        }
        #[cfg(feature = "hosted-baosec")]
        {
            let _ = sid;
            log::info!("power management disabled (hosted emulator)");
        }
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

    /// one animation tick: advance + draw, then check the idle timer
    fn on_pump(&mut self) {
        if self.idle {
            return;
        }
        self.step();
        self.draw();
        if self.power_active() {
            let now = self.tt.elapsed_ms();
            if now.saturating_sub(self.last_motion_ms) > IDLE_SECS * 1000 {
                self.enter_idle();
            }
        }
    }

    /// stop pumping + kill the screen; the kernel then parks the CPU in WFI
    fn enter_idle(&mut self) {
        log::info!("no motion for {}s -- idle (screen off)", IDLE_SECS);
        self.idle = true;
        if let Some(cid) = self.pump_cid {
            xous::send_message(
                cid,
                Message::new_scalar(PumpOp::Stop.to_usize().unwrap(), 0, 0, 0, 0),
            )
            .ok();
        }
        #[cfg(feature = "board-baosec")]
        self.gfx.set_power(false).ok();
    }

    /// accelerometer motion: refresh the idle timer; if asleep, wake up
    fn on_motion(&mut self) {
        #[cfg(not(feature = "hosted-baosec"))]
        if let Some(p) = self.power.as_mut() {
            p.clear_motion_irq();
        }
        self.last_motion_ms = self.tt.elapsed_ms();
        if self.idle {
            log::info!("motion detected -- waking up");
            self.idle = false;
            #[cfg(feature = "board-baosec")]
            self.gfx.set_power(true).ok();
            if let Some(cid) = self.pump_cid {
                xous::send_message(
                    cid,
                    Message::new_scalar(PumpOp::Run.to_usize().unwrap(), 0, 0, 0, 0),
                )
                .ok();
            }
            self.draw();
        }
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

/// background thread that paces the simulation. While running it sends a
/// blocking Pump every TICK_MS; when stopped it blocks on its control server
/// with no timers armed, letting the kernel park the CPU in WFI.
fn pump_thread(cid_to_main: xous::CID, pump_sid: xous::SID) {
    let _ = std::thread::spawn(move || {
        let tt = ticktimer_server::Ticktimer::new().unwrap();
        let cid_to_self = xous::connect(pump_sid).unwrap();
        let mut run = false;
        loop {
            let msg = xous::receive_message(pump_sid).unwrap();
            match FromPrimitive::from_usize(msg.body.id()) {
                Some(PumpOp::Run) => {
                    run = true;
                    xous::send_message(
                        cid_to_self,
                        Message::new_scalar(PumpOp::Pump.to_usize().unwrap(), 0, 0, 0, 0),
                    )
                    .ok();
                }
                Some(PumpOp::Stop) => run = false,
                Some(PumpOp::Pump) => {
                    xous::send_message(
                        cid_to_main,
                        Message::new_blocking_scalar(GolOp::Pump.to_usize().unwrap(), 0, 0, 0, 0),
                    )
                    .ok();
                    if run {
                        tt.sleep_ms(TICK_MS).unwrap();
                        xous::send_message(
                            cid_to_self,
                            Message::new_scalar(PumpOp::Pump.to_usize().unwrap(), 0, 0, 0, 0),
                        )
                        .ok();
                    }
                }
                Some(PumpOp::Quit) => break,
                _ => log::error!("unknown pump message: {:?}", msg),
            }
        }
        xous::destroy_server(pump_sid).ok();
    });
}

fn main() -> ! {
    log_server::init_wait().unwrap();
    log::set_max_level(log::LevelFilter::Info);
    log::info!("Game of Life PID is {}", xous::process::id());

    let xns = xous_names::XousNames::new().unwrap();
    let gfx = Gfx::new(&xns).expect("can't connect to GFX");
    let screensize = gfx.screen_size().expect("couldn't get screen size");
    log::info!("game of life screen: {}x{}", screensize.x, screensize.y);

    let sid = xns.register_name(GOL_SERVER_NAME, None).expect("can't register server");

    let mut gol = Gol::new(gfx, screensize);
    gol.draw(); // first paint

    // accelerometer motion wake (badge only; no-op on the emulator)
    gol.arm_power(sid);

    // pump thread: drive the animation; stopped while idle so the CPU sleeps
    let pump_sid = xous::create_server().unwrap();
    let cid_to_pump = xous::connect(pump_sid).unwrap();
    pump_thread(xous::connect(sid).unwrap(), pump_sid);
    gol.pump_cid = Some(cid_to_pump);
    xous::send_message(
        cid_to_pump,
        Message::new_scalar(PumpOp::Run.to_usize().unwrap(), 0, 0, 0, 0),
    )
    .ok();

    loop {
        let msg = xous::receive_message(sid).unwrap();
        match FromPrimitive::from_usize(msg.body.id()) {
            Some(GolOp::Pump) => {
                gol.on_pump();
                xous::return_scalar(msg.sender, 1).ok();
            }
            Some(GolOp::Motion) => gol.on_motion(),
            Some(GolOp::Quit) => break,
            _ => log::debug!("unknown message: {:?}", msg),
        }
    }
    log::info!("Game of Life quitting");
    xous::terminate_process(0)
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
