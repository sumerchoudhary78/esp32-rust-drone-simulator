use core::ops::{Add, Mul, Sub};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0., 0., 0.);
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    pub fn cross(self, b: Self) -> Self {
        Self::new(
            self.y * b.z - self.z * b.y,
            self.z * b.x - self.x * b.z,
            self.x * b.y - self.y * b.x,
        )
    }
    pub fn norm(self) -> f32 {
        libm::sqrtf(self.x * self.x + self.y * self.y + self.z * self.z)
    }
    pub fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}
impl Add for Vec3 {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self::new(self.x + b.x, self.y + b.y, self.z + b.z)
    }
}
impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self::new(self.x - b.x, self.y - b.y, self.z - b.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, b: f32) -> Self {
        Self::new(self.x * b, self.y * b, self.z * b)
    }
}

/// Body-to-world rotation. Body: X forward, Y left, Z up. Right handed.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Quaternion {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
impl Default for Quaternion {
    fn default() -> Self {
        Self {
            w: 1.,
            x: 0.,
            y: 0.,
            z: 0.,
        }
    }
}
impl Quaternion {
    pub fn from_euler(roll: f32, pitch: f32, yaw: f32) -> Self {
        let (sr, cr) = (libm::sinf(roll * 0.5), libm::cosf(roll * 0.5));
        let (sp, cp) = (libm::sinf(pitch * 0.5), libm::cosf(pitch * 0.5));
        let (sy, cy) = (libm::sinf(yaw * 0.5), libm::cosf(yaw * 0.5));
        Self {
            w: cr * cp * cy + sr * sp * sy,
            x: sr * cp * cy - cr * sp * sy,
            y: cr * sp * cy + sr * cp * sy,
            z: cr * cp * sy - sr * sp * cy,
        }
    }
    pub fn conjugate(self) -> Self {
        Self {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }
    pub fn rotate(self, v: Vec3) -> Vec3 {
        let q = Vec3::new(self.x, self.y, self.z);
        let t = q.cross(v) * 2.;
        v + t * self.w + q.cross(t)
    }
    pub fn integrate(&mut self, rate: Vec3, dt: f32) {
        let q = *self;
        self.w += (-q.x * rate.x - q.y * rate.y - q.z * rate.z) * dt * 0.5;
        self.x += (q.w * rate.x + q.y * rate.z - q.z * rate.y) * dt * 0.5;
        self.y += (q.w * rate.y + q.z * rate.x - q.x * rate.z) * dt * 0.5;
        self.z += (q.w * rate.z + q.x * rate.y - q.y * rate.x) * dt * 0.5;
        let n = libm::sqrtf(self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z);
        if n.is_finite() && n > 0.00001 {
            self.w /= n;
            self.x /= n;
            self.y /= n;
            self.z /= n;
        } else {
            *self = Self::default();
        }
    }
    pub fn euler(self) -> Vec3 {
        Vec3::new(
            libm::atan2f(
                2. * (self.w * self.x + self.y * self.z),
                1. - 2. * (self.x * self.x + self.y * self.y),
            ),
            libm::asinf((2. * (self.w * self.y - self.z * self.x)).clamp(-1., 1.)),
            libm::atan2f(
                2. * (self.w * self.z + self.x * self.y),
                1. - 2. * (self.y * self.y + self.z * self.z),
            ),
        )
    }
}
