use serde::{Deserialize, Serialize};

/// Normalized commands; roll/pitch request ±20°, yaw requests ±90°/s.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub sequence: u64,
    pub throttle: f32,
    pub roll: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub arm: bool,
}
impl Command {
    pub fn valid(&self) -> bool {
        self.throttle.is_finite()
            && (0.0..=1.0).contains(&self.throttle)
            && [self.roll, self.pitch, self.yaw]
                .iter()
                .all(|v| v.is_finite() && (-1.0..=1.0).contains(v))
    }
}
