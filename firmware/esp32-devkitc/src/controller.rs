use crate::{
    battery::{Battery, Calibration},
    commands::{Accepted, COMMAND_EXPIRY_US},
};
use flight_core::{
    FlightController,
    imu::{G, ImuSample},
    motors::MotorOutputs,
    safety::{FlightState, StopReason},
};
pub const PERIOD_US: u32 = 2_000;
pub const WATCHDOG_US: u32 = 10_000;
pub const STATIONARY_SAMPLES: u32 = 1500;
pub struct Controller {
    pub core: FlightController,
    pub voltage: Option<f32>,
    pub stationary: u32,
    pub switch_low_seen: bool,
    pub gate: bool,
    pub faults: u32,
    battery: Battery,
    last_command_us: Option<u32>,
    released: bool,
    max_pwm: u16,
}
impl Controller {
    pub fn new(calibration: Option<Calibration>, max_pwm: u16) -> Self {
        Self {
            core: FlightController::default(),
            voltage: None,
            stationary: 0,
            switch_low_seen: false,
            gate: false,
            faults: 0,
            battery: Battery::new(calibration),
            last_command_us: None,
            released: false,
            max_pwm: max_pwm.clamp(1000, 2000),
        }
    }
    pub fn reset_calibration(&mut self) {
        self.gate = false;
        self.released = false;
        self.stationary = 0;
        self.last_command_us = None;
        self.core = FlightController::default();
    }
    pub fn command(&mut self, p: Accepted, now_us: u32, switch_high: bool) {
        if now_us.wrapping_sub(p.received_us) >= COMMAND_EXPIRY_US {
            return;
        }
        if p.emergency {
            self.cut(StopReason::EmergencyStop);
            return;
        }
        if !p.command.arm {
            self.released = !switch_high && self.switch_low_seen;
        }
        if p.command.arm
            && (!self.released
                || !self.switch_low_seen
                || !switch_high
                || self.stationary < STATIONARY_SAMPLES
                || self.voltage.is_none()
                || !matches!(self.core.state, FlightState::Disarmed | FlightState::Armed))
        {
            self.released = false;
            return;
        }
        if self
            .core
            .accept_command(p.command, now_us as u64 / 1000)
            .is_ok()
        {
            self.last_command_us = Some(p.received_us);
        }
    }
    pub fn cut(&mut self, reason: StopReason) {
        self.gate = false;
        self.released = false;
        self.faults = self.faults.saturating_add(1);
        self.core.stop(reason);
    }
    pub fn step(
        &mut self,
        sample: Option<ImuSample>,
        adc: Option<u16>,
        switch_high: bool,
        now_us: u32,
    ) -> MotorOutputs {
        self.voltage = self.battery.update(adc);
        if !switch_high {
            self.switch_low_seen = true;
            if self.core.state == FlightState::Armed {
                self.cut(StopReason::EmergencyStop);
            }
        }
        if self.stationary < STATIONARY_SAMPLES {
            self.gate = false;
            let still = sample.is_some_and(|s| {
                s.finite()
                    && s.gyro.norm() < 0.10
                    && (0.95 * G..1.05 * G).contains(&s.accel.norm())
                    && s.accel.z > 0.98 * G
            });
            self.stationary = if still { self.stationary + 1 } else { 0 };
            return MotorOutputs::STOP;
        }
        if self.core.state == FlightState::Armed
            && (self
                .last_command_us
                .is_none_or(|t| now_us.wrapping_sub(t) >= COMMAND_EXPIRY_US)
                || self.voltage.is_none())
        {
            self.cut(if self.voltage.is_none() {
                StopReason::LowBattery
            } else {
                StopReason::LinkTimeout
            });
        }
        self.core.step(
            sample,
            self.voltage.unwrap_or(f32::NAN),
            now_us as u64 / 1000,
        );
        if self.core.state == FlightState::Failsafe {
            self.released = false;
        }
        self.gate = self.core.state == FlightState::Armed && switch_high && self.released;
        if !self.gate {
            return MotorOutputs::STOP;
        }
        let mut out = self.core.motors;
        for i in 0..4 {
            out.pwm_us[i] = out.pwm_us[i].min(self.max_pwm);
            out.normalized[i] = (out.pwm_us[i] - 1000) as f32 / 1000.;
        }
        out
    }
}
