#![no_std]
//! Hardware-independent, allocation-free flight logic shared by the simulator
//! and a future ESP32 adapter. Not flight-qualified firmware.
pub mod attitude;
pub mod commands;
pub mod imu;
pub mod math;
pub mod motors;
pub mod pid;
pub mod safety;

use attitude::AttitudeEstimator;
use commands::Command;
use imu::{G, ImuSample};
use math::Vec3;
use motors::MotorOutputs;
use pid::{Gains, Pid};
use safety::{ARM_VOLTAGE, COMMAND_TIMEOUT_MS, CRITICAL_VOLTAGE, FlightState, StopReason};
pub const LOOP_HZ: u32 = 500;
pub const DT: f32 = 1. / LOOP_HZ as f32;
pub const CALIBRATION_SAMPLES: u32 = 500;

pub struct FlightController {
    pub state: FlightState,
    pub reason: Option<StopReason>,
    pub attitude: Vec3,
    pub rates: Vec3,
    pub gyro_bias: Vec3,
    pub calibration_samples: u32,
    pub motors: MotorOutputs,
    pub gains: Gains,
    pub command: Command,
    estimator: AttitudeEstimator,
    pid: [Pid; 3],
    bias_sum: Vec3,
    last_command_ms: Option<u64>,
    last_sequence: Option<u64>,
    arm_was_high: bool,
}
impl Default for FlightController {
    fn default() -> Self {
        Self {
            state: FlightState::Calibrating,
            reason: None,
            attitude: Vec3::ZERO,
            rates: Vec3::ZERO,
            gyro_bias: Vec3::ZERO,
            calibration_samples: 0,
            motors: MotorOutputs::STOP,
            gains: Gains::default(),
            command: Command::default(),
            estimator: AttitudeEstimator::default(),
            pid: core::array::from_fn(|_| Pid::default()),
            bias_sum: Vec3::ZERO,
            last_command_ms: None,
            last_sequence: None,
            arm_was_high: false,
        }
    }
}
impl FlightController {
    /// Only a fresh valid sequence advances the watchdog. Time comes from the
    /// local monotonic clock, never from a timestamp supplied by a client.
    pub fn accept_command(
        &mut self,
        command: Command,
        received_ms: u64,
    ) -> Result<(), &'static str> {
        if !command.valid() {
            return Err("Command values must be finite and within their normalized ranges");
        }
        if command.arm && self.calibration_samples < CALIBRATION_SAMPLES {
            self.arm_was_high = true;
            return Err("Complete stationary IMU calibration before arming");
        }
        if self.last_sequence.is_some_and(|n| command.sequence <= n) {
            return Err("Stale or duplicate command sequence");
        }
        self.last_sequence = Some(command.sequence);
        self.last_command_ms = Some(received_ms);
        self.command = command;
        if !command.arm {
            self.arm_was_high = false;
            if self.state != FlightState::Calibrating {
                self.disarm();
            }
        }
        Ok(())
    }
    /// Called only on a new transport ownership epoch, with the motors stopped.
    pub fn release_control(&mut self) {
        self.last_sequence = None;
        self.last_command_ms = None;
        self.command = Command::default();
        self.arm_was_high = false;
        self.disarm();
    }
    pub fn command_age_ms(&self, now_ms: u64) -> Option<u64> {
        self.last_command_ms.map(|t| now_ms.saturating_sub(t))
    }
    pub fn stop(&mut self, reason: StopReason) {
        self.state = FlightState::Failsafe;
        self.reason = Some(reason);
        self.motors = MotorOutputs::STOP;
        for p in &mut self.pid {
            p.reset();
        }
    }
    pub fn disarm(&mut self) {
        if self.calibration_samples >= CALIBRATION_SAMPLES {
            self.state = FlightState::Disarmed;
            self.reason = None;
        }
        self.motors = MotorOutputs::STOP;
        for p in &mut self.pid {
            p.reset();
        }
    }
    pub fn tune(&mut self, gains: Gains) -> Result<(), &'static str> {
        if self.state == FlightState::Armed {
            return Err("Disarm before changing PID gains");
        }
        if !gains.valid() {
            return Err("PID gains outside allowed bounds");
        }
        self.gains = gains;
        for p in &mut self.pid {
            p.reset();
        }
        Ok(())
    }
    pub fn step(&mut self, sample: Option<ImuSample>, voltage: f32, now_ms: u64) {
        let Some(sample) = sample.filter(|s| s.finite()) else {
            if self.calibration_samples < CALIBRATION_SAMPLES {
                self.calibration_samples = 0;
                self.bias_sum = Vec3::ZERO;
            }
            if self.command.arm {
                self.arm_was_high = true;
            }
            if self.state == FlightState::Armed {
                self.stop(StopReason::ImuFailure);
            }
            self.motors = MotorOutputs::STOP;
            return;
        };
        if self.calibration_samples < CALIBRATION_SAMPLES {
            self.state = FlightState::Calibrating;
            self.motors = MotorOutputs::STOP;
            if sample.gyro.norm() < 0.15 && (0.92 * G..1.08 * G).contains(&sample.accel.norm()) {
                self.bias_sum = self.bias_sum + sample.gyro;
                self.calibration_samples += 1;
                if self.calibration_samples == CALIBRATION_SAMPLES {
                    self.gyro_bias = self.bias_sum * (1. / CALIBRATION_SAMPLES as f32);
                    self.estimator.align(sample.accel);
                    self.state = FlightState::Disarmed;
                }
            } else {
                self.calibration_samples = 0;
                self.bias_sum = Vec3::ZERO;
            }
            return;
        }
        self.rates = sample.gyro - self.gyro_bias;
        self.attitude = self.estimator.update(sample, self.gyro_bias, DT);
        let fresh = self
            .command_age_ms(now_ms)
            .is_some_and(|age| age < COMMAND_TIMEOUT_MS);
        let level = self.attitude.x.abs() < 0.26 && self.attitude.y.abs() < 0.26;
        if self.state == FlightState::Disarmed && self.command.arm && !self.arm_was_high {
            self.arm_was_high = true;
            if self.command.throttle <= 0.03
                && fresh
                && level
                && voltage.is_finite()
                && voltage >= ARM_VOLTAGE
            {
                self.state = FlightState::Armed;
            }
        }
        if self.state != FlightState::Armed {
            self.motors = MotorOutputs::STOP;
            return;
        }
        if !fresh {
            self.stop(StopReason::LinkTimeout);
            return;
        }
        if !voltage.is_finite() || voltage <= CRITICAL_VOLTAGE {
            self.stop(StopReason::LowBattery);
            return;
        }
        if self.attitude.x.abs() > 1.22 || self.attitude.y.abs() > 1.22 {
            self.stop(StopReason::ExcessiveTilt);
            return;
        }
        if self.command.throttle <= 0.03 {
            self.motors = MotorOutputs::from_normalized([0.06; 4]);
            for p in &mut self.pid {
                p.reset();
            }
            return;
        }
        let target = Vec3::new(
            ((self.command.roll * 0.349066 - self.attitude.x) * 5.).clamp(-2.5, 2.5),
            ((self.command.pitch * 0.349066 - self.attitude.y) * 5.).clamp(-2.5, 2.5),
            self.command.yaw * core::f32::consts::FRAC_PI_2,
        );
        let r = self.pid[0].update(target.x, self.rates.x, self.gains, DT);
        let p = self.pid[1].update(target.y, self.rates.y, self.gains, DT);
        let y = self.pid[2].update(
            target.z,
            self.rates.z,
            Gains {
                kp: self.gains.kp * 1.8,
                ki: self.gains.ki,
                kd: self.gains.kd,
            },
            DT,
        );
        self.motors = motors::mix(self.command.throttle, r, p, y);
    }
}
