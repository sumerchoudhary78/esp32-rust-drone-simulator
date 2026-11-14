use flight_core::{
    DT,
    imu::{G, ImuSample, decode_mpu6050},
    math::{Quaternion, Vec3},
};

pub const MASS_KG: f32 = 1.2;
pub const ARM_M: f32 = 0.159099;
pub const MAX_THRUST_N: f32 = 6.5;
pub const HOVER_THROTTLE: f32 = 0.672768;

pub struct Plant {
    pub position: Vec3,
    pub velocity: Vec3,
    pub orientation: Quaternion,
    pub rates: Vec3,
    pub motor_speeds: [f32; 4],
    pub acceleration: Vec3,
    pub charge: f32,
    rng: u64,
}
impl Default for Plant {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            orientation: Quaternion::default(),
            rates: Vec3::ZERO,
            motor_speeds: [0.; 4],
            acceleration: Vec3::ZERO,
            charge: 1.,
            rng: 0x676c696465,
        }
    }
}
impl Plant {
    fn noise(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        ((self.rng >> 40) as f32 / (1u32 << 24) as f32) * 2. - 1.
    }
    /// Generates specific force and biased/noisy gyro data, then quantizes them
    /// through the actual MPU6050 register format and shared decoder.
    pub fn imu(&mut self) -> ImuSample {
        let specific = self
            .orientation
            .conjugate()
            .rotate(self.acceleration + Vec3::new(0., 0., G));
        let accel = specific + Vec3::new(self.noise(), self.noise(), self.noise()) * 0.025;
        let gyro = self.rates
            + Vec3::new(0.008, -0.006, 0.004)
            + Vec3::new(self.noise(), self.noise(), self.noise()) * 0.002;
        let mut raw = [0u8; 14];
        let gyro_scale = 131. * 180. / std::f32::consts::PI;
        for (i, value) in [
            (0, accel.x / G * 16384.),
            (2, accel.y / G * 16384.),
            (4, accel.z / G * 16384.),
            (6, (32. - 36.53) * 340.),
            (8, gyro.x * gyro_scale),
            (10, gyro.y * gyro_scale),
            (12, gyro.z * gyro_scale),
        ] {
            raw[i..i + 2].copy_from_slice(
                &(value.clamp(i16::MIN as f32, i16::MAX as f32) as i16).to_be_bytes(),
            );
        }
        decode_mpu6050(raw)
    }
    pub fn voltage(&self) -> f32 {
        9.9 + 2.7 * self.charge - self.motor_speeds.iter().sum::<f32>() * 0.075
    }
    pub fn step(&mut self, outputs: [f32; 4], wind: f32, motor_failure: bool) -> bool {
        for (i, motor) in self.motor_speeds.iter_mut().enumerate() {
            let target = if motor_failure && i == 0 {
                0.
            } else {
                outputs[i]
            };
            *motor += (target - *motor) * (DT / 0.045);
        }
        let thrust = self.motor_speeds.map(|v| MAX_THRUST_N * v * v);
        let total = thrust.iter().sum::<f32>();
        let torque = Vec3::new(
            ARM_M * (thrust[0] - thrust[1] - thrust[2] + thrust[3]),
            ARM_M * (-thrust[0] - thrust[1] + thrust[2] + thrust[3]),
            0.025 * (thrust[0] - thrust[1] + thrust[2] - thrust[3]),
        );
        let inertia = Vec3::new(0.018, 0.018, 0.032);
        let angular_momentum = Vec3::new(
            inertia.x * self.rates.x,
            inertia.y * self.rates.y,
            inertia.z * self.rates.z,
        );
        let net = torque - self.rates.cross(angular_momentum) - self.rates * 0.025;
        self.rates =
            self.rates + Vec3::new(net.x / inertia.x, net.y / inertia.y, net.z / inertia.z) * DT;
        self.orientation.integrate(self.rates, DT);
        let drag = Vec3::new(wind - self.velocity.x, -self.velocity.y, -self.velocity.z) * 0.35;
        self.acceleration = (self.orientation.rotate(Vec3::new(0., 0., total)) + drag)
            * (1. / MASS_KG)
            - Vec3::new(0., 0., G);
        self.velocity = self.velocity + self.acceleration * DT;
        self.position = self.position + self.velocity * DT;
        let mut impact = false;
        if self.position.z <= 0. {
            impact = self.velocity.z < -3.;
            self.position.z = 0.;
            self.velocity.z = 0.;
            self.velocity.x *= 0.97;
            self.velocity.y *= 0.97;
            // A simplified flat-floor constraint, not a rigid contact solver.
            if total < MASS_KG * G {
                let e = self.orientation.euler();
                self.orientation = Quaternion::from_euler(e.x * 0.97, e.y * 0.97, e.z);
                self.rates = self.rates * 0.94;
                self.acceleration = Vec3::ZERO;
            }
        }
        self.charge = (self.charge
            - DT * self.motor_speeds.iter().map(|v| v * v).sum::<f32>() / 1000.)
            .max(0.);
        impact
    }
}
