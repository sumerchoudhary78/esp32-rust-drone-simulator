use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gains {
    pub kp: f32,
    pub ki: f32,
    pub kd: f32,
}
impl Default for Gains {
    fn default() -> Self {
        Self {
            kp: 0.07,
            ki: 0.025,
            kd: 0.0015,
        }
    }
}
impl Gains {
    pub fn valid(self) -> bool {
        self.kp.is_finite()
            && self.ki.is_finite()
            && self.kd.is_finite()
            && (0.0..=0.3).contains(&self.kp)
            && (0.0..=0.2).contains(&self.ki)
            && (0.0..=0.02).contains(&self.kd)
    }
}
#[derive(Default)]
pub struct Pid {
    integral: f32,
    last_measurement: Option<f32>,
    derivative: f32,
}
impl Pid {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    /// Derivative on measurement avoids setpoint kick; low-pass at ~30Hz.
    /// Conditional integration and an integral bound limit wind-up.
    pub fn update(&mut self, target: f32, measured: f32, gains: Gains, dt: f32) -> f32 {
        let error = target - measured;
        let raw_d = self.last_measurement.map_or(0., |v| (measured - v) / dt);
        self.last_measurement = Some(measured);
        self.derivative += (dt / (0.005 + dt)) * (raw_d - self.derivative);
        let proposed = (self.integral + error * dt * gains.ki).clamp(-0.12, 0.12);
        let unsaturated = error * gains.kp + proposed - self.derivative * gains.kd;
        if unsaturated.abs() < 0.25 || unsaturated * error < 0. {
            self.integral = proposed;
        }
        (error * gains.kp + self.integral - self.derivative * gains.kd).clamp(-0.25, 0.25)
    }
}
