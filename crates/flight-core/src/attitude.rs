use crate::{
    imu::{G, ImuSample},
    math::{Quaternion, Vec3},
};

#[derive(Default)]
pub struct AttitudeEstimator {
    pub orientation: Quaternion,
}
impl AttitudeEstimator {
    pub fn align(&mut self, accel: Vec3) {
        let roll = libm::atan2f(accel.y, accel.z);
        let pitch = libm::atan2f(-accel.x, libm::sqrtf(accel.y * accel.y + accel.z * accel.z));
        self.orientation = Quaternion::from_euler(roll, pitch, 0.);
    }
    /// Quaternion complementary filter. Accel correction is gated near 1g.
    /// A six-axis IMU has no absolute yaw reference and cannot separate all
    /// sustained translational acceleration from gravity.
    pub fn update(&mut self, sample: ImuSample, bias: Vec3, dt: f32) -> Vec3 {
        let mut rate = sample.gyro - bias;
        let norm = sample.accel.norm();
        if (0.85 * G..1.15 * G).contains(&norm) {
            let predicted = self.orientation.conjugate().rotate(Vec3::new(0., 0., 1.));
            rate = rate + (sample.accel * (1. / norm)).cross(predicted) * 0.35;
        }
        self.orientation.integrate(rate, dt);
        self.orientation.euler()
    }
}
