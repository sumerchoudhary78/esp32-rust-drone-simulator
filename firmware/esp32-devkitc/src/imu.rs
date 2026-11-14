//! MPU6050 configuration for a real 1 kHz sensor / 500 Hz control loop.
use embedded_hal::{delay::DelayNs, i2c::I2c};
use flight_core::{
    imu::{G, ImuSample},
    math::Vec3,
};
#[derive(Debug, PartialEq)]
pub enum Error<E> {
    Bus(E),
    Identity(u8),
    Configuration(u8),
    NotReady,
    Saturated,
}
pub struct Mpu6050<I> {
    bus: I,
    address: u8,
}
impl<I: I2c> Mpu6050<I> {
    pub fn new(bus: I, address: u8) -> Self {
        Self { bus, address }
    }
    fn read(&mut self, register: u8, data: &mut [u8]) -> Result<(), Error<I::Error>> {
        self.bus
            .write_read(self.address, &[register], data)
            .map_err(Error::Bus)
    }
    fn write(&mut self, register: u8, value: u8) -> Result<(), Error<I::Error>> {
        self.bus
            .write(self.address, &[register, value])
            .map_err(Error::Bus)
    }
    pub fn initialize(&mut self, delay: &mut impl DelayNs) -> Result<(), Error<I::Error>> {
        let mut id = [0];
        self.read(0x75, &mut id)?;
        if id[0] != 0x68 {
            return Err(Error::Identity(id[0]));
        }
        self.write(0x6b, 0x80)?;
        delay.delay_ms(100);
        // PLL gyro X, all axes enabled, FIFO/DMP disabled, 98 Hz gyro DLPF,
        // 1 kHz output, ±2000 dps and ±8 g; INT_STATUS gates fresh reads.
        for (reg, value) in [
            (0x6b, 1),
            (0x6c, 0),
            (0x6a, 0),
            (0x23, 0),
            (0x19, 0),
            (0x1a, 2),
            (0x1b, 0x18),
            (0x1c, 0x10),
            (0x38, 1),
        ] {
            self.write(reg, value)?;
        }
        delay.delay_ms(100);
        for (reg, expected) in [(0x6b, 1), (0x19, 0), (0x1a, 2), (0x1b, 0x18), (0x1c, 0x10)] {
            let mut value = [0];
            self.read(reg, &mut value)?;
            if value[0] != expected {
                return Err(Error::Configuration(reg));
            }
        }
        Ok(())
    }
    pub fn sample(&mut self) -> Result<ImuSample, Error<I::Error>> {
        let mut status = [0];
        self.read(0x3a, &mut status)?;
        if status[0] & 1 == 0 {
            return Err(Error::NotReady);
        }
        let mut bytes = [0; 14];
        self.read(0x3b, &mut bytes)?;
        let n = |i| i16::from_be_bytes([bytes[i], bytes[i + 1]]);
        if [0, 2, 4, 8, 10, 12]
            .iter()
            .any(|i| i32::from(n(*i)).abs() >= 32700)
        {
            return Err(Error::Saturated);
        }
        // Sensor +X forward, +Y left, +Z up is the fixed board mounting.
        // Physical axis/sign checks are required before enabling a vehicle.
        Ok(ImuSample {
            accel: Vec3::new(n(0) as f32, n(2) as f32, n(4) as f32) * (G / 4096.),
            gyro: Vec3::new(n(8) as f32, n(10) as f32, n(12) as f32)
                * (core::f32::consts::PI / (180. * 16.4)),
            temperature_c: n(6) as f32 / 340. + 36.53,
        })
    }
    pub fn into_inner(self) -> I {
        self.bus
    }
}
