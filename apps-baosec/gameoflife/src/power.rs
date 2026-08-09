//! Battery-saving idle/wake support, mirroring the stock DC34 badge firmware
//! (dc34-console's power manager): the LIS2DH12 accelerometer's motion
//! interrupt is configured and routed to this app; once armed, the app's idle
//! loop can stop all timers and let the kernel park the CPU in WFI until a
//! motion interrupt wakes it.
//!
//! Only compiled for the real badge; the hosted emulator has no accelerometer.

use bao1x_api::{IoIrq, IoxHal, IoxPort, IoxValue};
use bao1x_hal::i2c::I2c;
use bao1x_hal::lis2dh12::{regs, InterruptPolarity, Lis2dh12};
use xous::SID;

/// the badge's accelerometer INT1 (motion) output pin, per dc34-console
const ACCEL_INT_PORT: IoxPort = IoxPort::PC;
const ACCEL_INT_PIN: u8 = 15;

pub struct Power {
    accel: Lis2dh12,
    i2c: I2c,
}

impl Power {
    /// Initialize the accelerometer, arm the motion interrupt and route it to
    /// `server` with `opcode`. Returns None (disabling idle power management)
    /// if no accelerometer responds on the I2C bus.
    pub fn new(server: SID, opcode: usize) -> Option<Self> {
        let mut i2c = I2c::new();
        let mut accel = match Lis2dh12::new(&mut i2c) {
            Ok(a) => a,
            Err(_) => {
                log::warn!("no accelerometer found; power management disabled");
                return None;
            }
        };

        // ---- mirror the stock firmware's wake tuning (dc34-console) ----
        let saved_ctrl3 = accel.read_register(&mut i2c, regs::CTRL_REG3).unwrap_or(0);
        // latch INT1 until its SRC register is read
        accel.write_register(&mut i2c, regs::CTRL_REG5, 0x08).ok();
        // INT1: OR combination, all axes high/low
        accel.write_register(&mut i2c, regs::INT1_CFG, 0x7F).ok();
        // 25 Hz, normal mode, XYZ enabled
        accel.write_register(&mut i2c, regs::CTRL_REG1, 0x37).ok();
        // ~18 mg threshold, 120 ms minimum duration (rejects transients)
        accel.write_register(&mut i2c, regs::INT1_THS, 18).ok();
        accel.write_register(&mut i2c, regs::INT1_DURATION, 3).ok();
        // high-pass filter on INT1: sustained tilt does not re-trigger
        accel.write_register(&mut i2c, regs::CTRL_REG2, 0x01).ok();
        accel.read_register(&mut i2c, regs::REFERENCE).ok();
        // route IA1 (motion) to the INT1 pin
        accel.write_register(&mut i2c, regs::CTRL_REG3, saved_ctrl3 | 0x40).ok();
        accel.set_interrupt_polarity(&mut i2c, InterruptPolarity::ActiveHigh).ok();
        // clear any pending latches before arming the pin
        let _ = accel.read_register(&mut i2c, regs::INT1_SRC);

        // route the INT1 pin interrupt to our server
        let iox = IoxHal::new();
        let server_name = crate::GOL_SERVER_NAME;
        iox.set_irq_pin(ACCEL_INT_PORT, ACCEL_INT_PIN, IoxValue::Low, server_name, opcode);
        log::info!("motion wake armed on PC{}", ACCEL_INT_PIN);

        Some(Power { accel, i2c })
    }

    /// The motion IRQ fired: read INT1_SRC to clear the latched interrupt so
    /// it can fire again on the next movement.
    pub fn clear_motion_irq(&mut self) {
        let _ = self.accel.read_register(&mut self.i2c, regs::INT1_SRC);
    }
}
