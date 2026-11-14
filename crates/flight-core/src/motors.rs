use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct MotorOutputs {
    pub normalized: [f32; 4],
    pub pwm_us: [u16; 4],
}
impl MotorOutputs {
    pub const STOP: Self = Self {
        normalized: [0.; 4],
        pwm_us: [1000; 4],
    };
    pub fn from_normalized(values: [f32; 4]) -> Self {
        let normalized = values.map(|v| if v.is_finite() { v.clamp(0., 1.) } else { 0. });
        Self {
            normalized,
            pwm_us: normalized.map(|v| (1000. + v * 1000.) as u16),
        }
    }
}
/// X mixer: FL(CW), FR(CCW), RR(CW), RL(CCW), viewed from above.
pub fn mix(throttle: f32, roll: f32, pitch: f32, yaw: f32) -> MotorOutputs {
    let mut values = [
        throttle + roll - pitch + yaw,
        throttle - roll - pitch - yaw,
        throttle - roll + pitch + yaw,
        throttle + roll + pitch - yaw,
    ];
    // Shift collective first to preserve torque authority where possible.
    let max = values.iter().copied().fold(0., f32::max);
    if max > 1. {
        for v in &mut values {
            *v -= max - 1.;
        }
    }
    let min = values.iter().copied().fold(1., f32::min);
    if min < 0. {
        for v in &mut values {
            *v -= min;
        }
    }
    MotorOutputs::from_normalized(values)
}
