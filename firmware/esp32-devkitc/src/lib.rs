#![no_std]
//! Milestone 1 boot diagnostics. No sensor data or flight commands are invented.
//! The GPIO driver in main independently holds every reserved motor pin LOW.
use flight_core::{FlightController, motors::MotorOutputs, safety::FlightState};

pub const BOARD: &str = "ESP32-DevKitC-32E";
pub const CHIP: &str = "esp32";
pub const FLASH_BYTES: u32 = 4 * 1024 * 1024;
pub const MOTOR_GPIOS: [u8; 4] = [25, 26, 27, 14];
pub const UART_BAUD: u32 = 115_200;

#[derive(Debug)]
pub struct BootReport {
    pub uptime_ms: u64,
    pub heartbeat: u64,
    pub controller_state: FlightState,
    pub motor_requests: MotorOutputs,
}

#[derive(Default)]
pub struct Bootstrap {
    // No command receiver is exposed by this boot-only milestone.
    controller: FlightController,
    heartbeats: u64,
}

impl Bootstrap {
    /// Observe the actual unready controller: no IMU driver and no calibrated
    /// voltage source exist yet. This is a 1 Hz diagnostic, not a flight loop.
    pub fn heartbeat(&mut self, uptime_ms: u64) -> BootReport {
        self.controller.step(None, f32::NAN, uptime_ms);
        self.heartbeats += 1;
        BootReport {
            uptime_ms,
            heartbeat: self.heartbeats,
            controller_state: self.controller.state,
            motor_requests: self.controller.motors,
        }
    }
}
