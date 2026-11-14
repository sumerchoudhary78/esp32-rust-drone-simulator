pub mod physics;
pub mod server;
use flight_core::{
    CALIBRATION_SAMPLES, DT, FlightController,
    commands::Command,
    imu::ImuSample,
    math::{Quaternion, Vec3},
    motors::MotorOutputs,
    pid::Gains,
    safety::{FlightState, StopReason},
};
use physics::{MASS_KG, MAX_THRUST_N, Plant};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub altitude_assist: bool,
    pub target_altitude: f32,
    pub latency_ms: u32,
    pub packet_loss: f32,
    pub wind: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            altitude_assist: true,
            target_altitude: 1.5,
            latency_ms: 20,
            packet_loss: 0.,
            wind: 0.,
        }
    }
}
impl Settings {
    pub fn valid(self) -> bool {
        self.target_altitude.is_finite()
            && (0.5..=5.).contains(&self.target_altitude)
            && self.latency_ms <= 800
            && self.packet_loss.is_finite()
            && (0.0..=100.).contains(&self.packet_loss)
            && self.wind.is_finite()
            && (0.0..=5.).contains(&self.wind)
    }
}
#[derive(Clone, Copy, Default, Debug, Serialize)]
pub struct Faults {
    pub link_loss: bool,
    pub imu_failure: bool,
    pub low_battery: bool,
    pub motor_failure: bool,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    Command {
        sequence: u64,
        throttle: f32,
        roll: f32,
        pitch: f32,
        yaw: f32,
        arm: bool,
    },
    Action {
        action: Action,
    },
    Fault {
        fault: Fault,
        enabled: bool,
    },
    Settings {
        altitude_assist: bool,
        target_altitude: f32,
        latency_ms: u32,
        packet_loss: f32,
        wind: f32,
    },
    Tune {
        kp: f32,
        ki: f32,
        kd: f32,
    },
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Reset,
    EmergencyStop,
    Takeoff,
    Land,
    Disturb,
    ClaimControl,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fault {
    LinkLoss,
    ImuFailure,
    LowBattery,
    MotorFailure,
}
#[derive(Clone, Debug, Serialize)]
pub struct FlightEvent {
    pub id: u64,
    pub time_s: f32,
    pub level: &'static str,
    pub message: String,
}
#[derive(Clone, Serialize)]
pub struct Telemetry {
    pub r#type: &'static str,
    pub tick: u64,
    pub sim_time_s: f32,
    pub state: FlightState,
    pub reason: Option<StopReason>,
    pub calibration_progress: f32,
    pub attitude_deg: Vec3,
    pub truth_attitude_deg: Vec3,
    pub orientation: Quaternion,
    pub rates_deg_s: Vec3,
    pub position_m: Vec3,
    pub velocity_m_s: Vec3,
    pub imu: ImuSample,
    pub motors: MotorOutputs,
    pub motor_actual: [f32; 4],
    pub battery_voltage: f32,
    pub battery_percent: f32,
    pub command: Command,
    pub command_age_ms: Option<u64>,
    pub settings: Settings,
    pub faults: Faults,
    pub gains: Gains,
    pub controller_id: Option<u64>,
    pub phase: &'static str,
    pub packets_received: u64,
    pub packets_dropped: u64,
    pub events: Vec<FlightEvent>,
    pub nominal_loop_hz: u32,
    pub measured_loop_hz: f32,
    pub timing_overruns: u64,
}
struct Packet {
    command: Command,
    received_ms: u64,
    due_ms: u64,
}
pub struct World {
    pub core: FlightController,
    pub plant: Plant,
    pub settings: Settings,
    pub faults: Faults,
    pub tick: u64,
    pub controller_id: Option<u64>,
    pub measured_loop_hz: f32,
    pub timing_overruns: u64,
    pending: VecDeque<Packet>,
    highest_sequence: Option<u64>,
    flying: bool,
    landing: bool,
    must_release_arm: bool,
    last_imu: ImuSample,
    events: VecDeque<FlightEvent>,
    event_id: u64,
    received: u64,
    dropped: u64,
    rng: u64,
}
impl Default for World {
    fn default() -> Self {
        let mut plant = Plant::default();
        let last_imu = plant.imu();
        let mut world = Self {
            core: FlightController::default(),
            plant,
            settings: Settings::default(),
            faults: Faults::default(),
            tick: 0,
            controller_id: None,
            measured_loop_hz: 500.,
            timing_overruns: 0,
            pending: VecDeque::new(),
            highest_sequence: None,
            flying: false,
            landing: false,
            must_release_arm: false,
            last_imu,
            events: VecDeque::new(),
            event_id: 0,
            received: 0,
            dropped: 0,
            rng: 1234567,
        };
        world.event(
            "info",
            "Virtual ESP32 booted · calibrating stationary MPU6050",
        );
        world
    }
}
impl World {
    pub fn now_ms(&self) -> u64 {
        self.tick * 2
    }
    pub fn event(&mut self, level: &'static str, message: impl Into<String>) {
        self.event_id += 1;
        self.events.push_front(FlightEvent {
            id: self.event_id,
            time_s: self.tick as f32 * DT,
            level,
            message: message.into(),
        });
        self.events.truncate(40);
    }
    pub fn connect(&mut self, id: u64) {
        if self.controller_id.is_none() {
            self.claim(id);
        }
    }
    pub fn disconnect(&mut self, id: u64) {
        if self.controller_id == Some(id) {
            self.controller_id = None;
            self.pending.clear();
            self.highest_sequence = None;
            self.event(
                "warn",
                "Controller disconnected · command watchdog remains active",
            );
        }
    }
    fn claim(&mut self, id: u64) {
        self.pending.clear();
        self.highest_sequence = None;
        self.flying = false;
        self.landing = false;
        self.must_release_arm = true;
        self.core.release_control();
        self.controller_id = Some(id);
        self.event(
            "info",
            format!("Controller {id} connected · motors disarmed"),
        );
    }
    pub fn handle(&mut self, id: u64, message: ClientMessage) -> Result<(), &'static str> {
        if matches!(
            message,
            ClientMessage::Action {
                action: Action::ClaimControl
            }
        ) {
            self.claim(id);
            return Ok(());
        }
        if self.controller_id != Some(id) {
            return Err("Observer mode · take control to send commands");
        }
        match message {
            ClientMessage::Command {
                sequence,
                throttle,
                roll,
                pitch,
                yaw,
                arm,
            } => self.receive(Command {
                sequence,
                throttle,
                roll,
                pitch,
                yaw,
                arm,
            }),
            ClientMessage::Settings {
                altitude_assist,
                target_altitude,
                latency_ms,
                packet_loss,
                wind,
            } => {
                let settings = Settings {
                    altitude_assist,
                    target_altitude,
                    latency_ms,
                    packet_loss,
                    wind,
                };
                if !settings.valid() {
                    return Err("Simulation settings outside allowed bounds");
                }
                if self.core.state == FlightState::Armed
                    && altitude_assist != self.settings.altitude_assist
                {
                    return Err("Disarm before changing throttle mode");
                }
                self.settings = settings;
                self.event("info", "Simulation settings updated");
                Ok(())
            }
            ClientMessage::Tune { kp, ki, kd } => {
                self.core.tune(Gains { kp, ki, kd })?;
                self.event("info", "Rate PID gains updated");
                Ok(())
            }
            ClientMessage::Fault { fault, enabled } => {
                let label = match fault {
                    Fault::LinkLoss => {
                        self.faults.link_loss = enabled;
                        "Wi-Fi link loss"
                    }
                    Fault::ImuFailure => {
                        self.faults.imu_failure = enabled;
                        "IMU read failure"
                    }
                    Fault::LowBattery => {
                        self.faults.low_battery = enabled;
                        "Critical battery"
                    }
                    Fault::MotorFailure => {
                        self.faults.motor_failure = enabled;
                        "M1 motor failure"
                    }
                };
                if self.faults.link_loss {
                    self.pending.clear();
                }
                self.event(
                    if enabled { "warn" } else { "info" },
                    format!("{label} {}", if enabled { "injected" } else { "cleared" }),
                );
                Ok(())
            }
            ClientMessage::Action { action } => self.action(action),
        }
    }
    pub fn receive(&mut self, command: Command) -> Result<(), &'static str> {
        if !command.valid() {
            return Err("Command values outside normalized ranges");
        }
        if self.highest_sequence.is_some_and(|n| command.sequence <= n) {
            return Err("Stale or duplicate command sequence");
        }
        self.highest_sequence = Some(command.sequence);
        self.received += 1;
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        let random = (self.rng % 10000) as f32 / 100.;
        if self.faults.link_loss || random < self.settings.packet_loss || self.pending.len() >= 64 {
            self.dropped += 1;
            return Ok(());
        }
        let received_ms = self.now_ms();
        self.pending.push_back(Packet {
            command,
            received_ms,
            due_ms: received_ms + self.settings.latency_ms as u64,
        });
        Ok(())
    }
    pub fn action(&mut self, action: Action) -> Result<(), &'static str> {
        match action {
            Action::Reset => {
                let id = self.controller_id;
                let event_id = self.event_id;
                *self = Self::default();
                self.controller_id = id;
                self.event_id = event_id;
                self.events.clear();
                self.must_release_arm = true;
                self.event("info", "Workspace reset · calibrating stationary MPU6050");
            }
            Action::EmergencyStop => {
                self.pending.clear();
                self.flying = false;
                self.landing = false;
                self.must_release_arm = true;
                self.core.stop(StopReason::EmergencyStop);
                self.event(
                    "danger",
                    "Emergency stop · all virtual ESCs commanded to 1000 µs",
                );
            }
            Action::Takeoff => {
                if self.core.state != FlightState::Armed {
                    return Err("Arm at zero throttle before takeoff");
                }
                if !self.settings.altitude_assist {
                    return Err("Takeoff helper requires virtual altitude assistance");
                }
                self.flying = true;
                self.landing = false;
                self.event("info", "Taking off · virtual height assistance engaged");
            }
            Action::Land => {
                if self.core.state != FlightState::Armed || !self.flying {
                    return Err("Take off before using assisted landing");
                }
                self.landing = true;
                self.event("info", "Descending · virtual landing helper engaged");
            }
            Action::Disturb => {
                if self.plant.position.z < 0.2 || self.core.state != FlightState::Armed {
                    return Err("Take off before applying a disturbance");
                }
                self.plant.rates = self.plant.rates + Vec3::new(1.1, -0.7, 0.25);
                self.event(
                    "warn",
                    "Angular impulse applied · observing stabilization response",
                );
            }
            Action::ClaimControl => {}
        }
        Ok(())
    }
    fn assist_throttle(&self) -> f32 {
        let target = if self.landing {
            0.
        } else {
            self.settings.target_altitude
        };
        let speed = ((target - self.plant.position.z) * 1.3).clamp(-0.6, 0.9);
        let acceleration = ((speed - self.plant.velocity.z) * 2.5).clamp(-3., 3.);
        let up = self
            .plant
            .orientation
            .rotate(Vec3::new(0., 0., 1.))
            .z
            .max(0.5);
        (MASS_KG * (flight_core::imu::G + acceleration) / (4. * MAX_THRUST_N * up))
            .sqrt()
            .clamp(0.05, 0.9)
    }
    pub fn step(&mut self) {
        let before = self.core.state;
        while self
            .pending
            .front()
            .is_some_and(|p| p.due_ms <= self.now_ms())
        {
            let packet = self.pending.pop_front().unwrap();
            let mut command = packet.command;
            if self.now_ms().saturating_sub(packet.received_ms) >= 500 {
                self.dropped += 1;
                continue;
            }
            if !command.arm {
                self.must_release_arm = false;
                self.flying = false;
                self.landing = false;
            }
            if self.must_release_arm && command.arm {
                continue;
            }
            if command.arm && self.settings.altitude_assist && self.flying {
                command.throttle = self.assist_throttle();
            }
            if self
                .core
                .accept_command(command, packet.received_ms)
                .is_err()
            {
                self.dropped += 1;
            }
        }
        let sample = if self.faults.imu_failure {
            None
        } else {
            let imu = self.plant.imu();
            self.last_imu = imu;
            Some(imu)
        };
        let voltage = if self.faults.low_battery {
            9.9
        } else {
            self.plant.voltage()
        };
        self.core.step(sample, voltage, self.now_ms());
        if self.core.state == FlightState::Failsafe {
            self.flying = false;
            self.landing = false;
        }
        if self.plant.step(
            self.core.motors.normalized,
            self.settings.wind,
            self.faults.motor_failure,
        ) {
            self.event("danger", "Ground impact · simplified collision model");
        }
        if self.landing && self.plant.position.z < 0.025 && self.plant.velocity.z.abs() < 0.15 {
            self.pending.clear();
            self.flying = false;
            self.landing = false;
            self.must_release_arm = true;
            self.core.disarm();
            self.event("info", "Landing complete · motors disarmed");
        }
        if before != self.core.state {
            let text = match self.core.state {
                FlightState::Calibrating => "Calibrating IMU".into(),
                FlightState::Disarmed => {
                    "Preflight ready · IMU calibrated · motors disarmed".into()
                }
                FlightState::Armed => "Armed · virtual motors at idle".into(),
                FlightState::Failsafe => format!(
                    "Failsafe latched · {:?} · motors cut",
                    self.core.reason.unwrap()
                ),
            };
            self.event(
                if self.core.state == FlightState::Failsafe {
                    "danger"
                } else {
                    "info"
                },
                text,
            );
        }
        self.tick += 1;
    }
    pub fn timing_overrun(&mut self) {
        self.timing_overruns += 1;
        if self.core.state == FlightState::Armed {
            self.pending.clear();
            self.must_release_arm = true;
            self.flying = false;
            self.core.stop(StopReason::TimingOverrun);
            self.event("danger", "Host scheduling overrun · virtual motors cut");
        }
    }
    pub fn snapshot(&self) -> Telemetry {
        let degrees = 180. / std::f32::consts::PI;
        Telemetry {
            r#type: "telemetry",
            tick: self.tick,
            sim_time_s: self.tick as f32 * DT,
            state: self.core.state,
            reason: self.core.reason,
            calibration_progress: self.core.calibration_samples as f32 / CALIBRATION_SAMPLES as f32,
            attitude_deg: self.core.attitude * degrees,
            truth_attitude_deg: self.plant.orientation.euler() * degrees,
            orientation: self.plant.orientation,
            rates_deg_s: self.core.rates * degrees,
            position_m: self.plant.position,
            velocity_m_s: self.plant.velocity,
            imu: self.last_imu,
            motors: self.core.motors,
            motor_actual: self.plant.motor_speeds,
            battery_voltage: if self.faults.low_battery {
                9.9
            } else {
                self.plant.voltage()
            },
            battery_percent: self.plant.charge * 100.,
            command: self.core.command,
            command_age_ms: self.core.command_age_ms(self.now_ms()),
            settings: self.settings,
            faults: self.faults,
            gains: self.core.gains,
            controller_id: self.controller_id,
            phase: if self.core.state == FlightState::Failsafe {
                "failsafe"
            } else if self.landing {
                "landing"
            } else if self.flying {
                "flying"
            } else {
                "grounded"
            },
            packets_received: self.received,
            packets_dropped: self.dropped,
            events: self.events.iter().cloned().collect(),
            nominal_loop_hz: 500,
            measured_loop_hz: self.measured_loop_hz,
            timing_overruns: self.timing_overruns,
        }
    }
}
