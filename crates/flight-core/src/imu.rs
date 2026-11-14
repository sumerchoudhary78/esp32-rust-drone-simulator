use crate::math::Vec3;
use serde::Serialize;
pub const G: f32 = 9.80665;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ImuSample {
    /// Specific force (m/s²), +Z at rest; NOT a gravity-free acceleration.
    pub accel: Vec3,
    /// Body angular velocity, rad/s.
    pub gyro: Vec3,
    pub temperature_c: f32,
}
impl ImuSample {
    pub fn finite(self) -> bool {
        self.accel.finite() && self.gyro.finite() && self.temperature_c.is_finite()
    }
}

/// Decode MPU6050 register 0x3B..0x48, configured for ±2g and ±250°/s.
/// The hardware adapter must rotate the sensor's axes into the body frame.
pub fn decode_mpu6050(bytes: [u8; 14]) -> ImuSample {
    let word = |i| i16::from_be_bytes([bytes[i], bytes[i + 1]]) as f32;
    let gyro_scale = core::f32::consts::PI / (180. * 131.);
    ImuSample {
        accel: Vec3::new(word(0), word(2), word(4)) * (G / 16384.),
        gyro: Vec3::new(word(8), word(10), word(12)) * gyro_scale,
        temperature_c: word(6) / 340. + 36.53,
    }
}
