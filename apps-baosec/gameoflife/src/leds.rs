//! WS2812 color wheel for the badge's LED chain, run from a background
//! thread while the Game of Life simulates on the main loop.
//!
//! The DC34 badge drives its LEDs through the BIO subsystem: bio-lib's
//! `Ws2812` driver claims one of the PicoRV I/O cores (via the BIO server in
//! bao1x-hal-service), loads a WS2812 kernel onto it, and streams GRB-packed
//! pixels through FIFO1. The chip pin and LED count come from the official
//! badge firmware (console app `leds.rs`):
//!
//! - pin: chip GPIO 15
//! - 10 LEDs on the human carrier: 8 x SK6812 (front strip) + 2 x
//!   WS2812B-2020 (the "eyes" on the back), all daisy-chained on one data
//!   line; the exhibitor carrier has 18.
//!
//! The color wheel is a port of the classic HSV rainbow from bio-lib's C
//! `colorwheel` example: hues spaced evenly across the chain, rotating one
//! step per tick, kept dim (V=64) because the badge runs on 2xAA batteries.
//!
//! The wheel pauses (and the LEDs go dark) while the badge is idle; the main
//! app sets `IDLE` when it parks the CPU to save power.

use arbitrary_int::u5;
use bio_lib::ws2812::{rgb_to_u32, LedVariant, Ws2812};
use std::sync::atomic::{AtomicBool, Ordering};

/// chip GPIO the LED chain's data line is wired to (from the official badge
/// firmware's `Lightgenes::new(u5::new(15), ...)`)
const LED_PIN: u5 = u5::new(15);
/// 8 front SK6812 + 2 back WS2812B eyes; the exhibitor carrier has 18
const LED_COUNT: usize = 10;
/// ms per wheel step; hue advances 1/256 per tick (~8.5 s per full rotation)
const WHEEL_TICK_MS: u64 = 33;
/// HSV value (brightness); kept low for battery life, matching the official
/// colorwheel example (V=64)
const BRIGHTNESS: u8 = 64;
/// saturation for the wheel
const SATURATION: u8 = 200;

/// set by the main app when it goes idle (screen off, CPU parked in WFI);
/// the wheel thread turns the LEDs off and stops spinning
pub static IDLE: AtomicBool = AtomicBool::new(false);

/// HSV -> RGB, ported from bio-lib/src/c/colorwheel/main.c (h in 0..=255)
fn hsv_to_rgb(h: u8, s: u8, v: u8) -> (u8, u8, u8) {
    if s == 0 {
        return (v, v, v);
    }
    let region = h / 43;
    let remainder = (h - region * 43) as u16 * 6;
    let p = (v as u16 * (255 - s as u16)) >> 8;
    let q = (v as u16 * (255 - ((s as u16 * remainder) >> 8))) >> 8;
    let t = (v as u16 * (255 - ((s as u16 * (255 - remainder)) >> 8))) >> 8;
    let (r, g, b) = match region {
        0 => (v as u16, t, p),
        1 => (q, v as u16, p),
        2 => (p, v as u16, t),
        3 => (p, q, v as u16),
        4 => (t, p, v as u16),
        _ => (v as u16, p, q),
    };
    (r as u8, g as u8, b as u8)
}

/// one wheel frame: hues spaced evenly across the chain, packed GRB
fn wheel_frame(hue: u8) -> [u32; LED_COUNT] {
    let mut frame = [0u32; LED_COUNT];
    let spacing = 256 / LED_COUNT as u16;
    for (i, word) in frame.iter_mut().enumerate() {
        let h = (hue as u16 + spacing * i as u16) % 256;
        let (r, g, b) = hsv_to_rgb(h as u8, SATURATION, BRIGHTNESS);
        *word = rgb_to_u32(r, g, b);
    }
    frame
}

/// spawn the wheel thread. Fails gracefully (logs + no LEDs) if the BIO
/// resources can't be claimed, so the game still runs on systems without
/// the LED chain (e.g. the hosted emulator). The driver must be constructed
/// here: it holds raw CSR pointers and is not `Send`.
pub fn start() {
    std::thread::spawn(move || {
        let mut strip = match Ws2812::new(LedVariant::B, LED_PIN, None) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("LEDs unavailable (no BIO resources?): {:?}", e);
                return;
            }
        };
        log::info!(
            "LED color wheel started: pin {}, {} LEDs, {} ms/tick",
            LED_PIN.value(),
            LED_COUNT,
            WHEEL_TICK_MS
        );
        let mut hue: u8 = 0;
        let mut dark = false;
        loop {
            if IDLE.load(Ordering::Relaxed) {
                if !dark {
                    // one all-off frame, then stop driving the chain
                    strip.send(&[0u32; LED_COUNT]);
                    dark = true;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            }
            dark = false;
            let frame = wheel_frame(hue);
            // the kernel transmits slice[0] first, and the first value sent
            // lands on the *last* LED of the chain, so hand it the reverse
            let mut rev = [0u32; LED_COUNT];
            for (i, w) in frame.iter().enumerate() {
                rev[LED_COUNT - 1 - i] = *w;
            }
            strip.send(&rev);
            hue = hue.wrapping_add(1);
            std::thread::sleep(std::time::Duration::from_millis(WHEEL_TICK_MS));
        }
    });
}
