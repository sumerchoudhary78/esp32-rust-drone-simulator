use aerolab_esp32_devkitc::imu::{Error, Mpu6050};
use embedded_hal::{
    delay::DelayNs,
    i2c::{ErrorKind, ErrorType, I2c, Operation},
};
use flight_core::imu::G;
#[derive(Debug, Copy, Clone, PartialEq)]
struct BusError;
impl embedded_hal::i2c::Error for BusError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}
struct Bus {
    regs: [u8; 128],
    nack: bool,
    bad_readback: bool,
}
impl Bus {
    fn new() -> Self {
        let mut s = Self {
            regs: [0; 128],
            nack: false,
            bad_readback: false,
        };
        s.regs[0x75] = 0x68;
        s
    }
}
impl ErrorType for Bus {
    type Error = BusError;
}
impl I2c for Bus {
    fn transaction(&mut self, address: u8, ops: &mut [Operation<'_>]) -> Result<(), BusError> {
        assert_eq!(address, 0x68);
        if self.nack {
            return Err(BusError);
        }
        let mut cursor = 0;
        for op in ops {
            match op {
                Operation::Write(bytes) => {
                    cursor = bytes[0] as usize;
                    for b in &bytes[1..] {
                        self.regs[cursor] = *b;
                        cursor += 1;
                    }
                }
                Operation::Read(bytes) => {
                    for b in bytes.iter_mut() {
                        *b = if self.bad_readback && cursor == 0x1b {
                            0
                        } else {
                            self.regs[cursor]
                        };
                        cursor += 1;
                    }
                }
            }
        }
        Ok(())
    }
}
#[derive(Default)]
struct Delay(u32);
impl DelayNs for Delay {
    fn delay_ns(&mut self, n: u32) {
        self.0 += n / 1_000_000;
    }
}
#[test]
fn initializes_real_ranges_pll_filter_and_disables_fifo_with_readback() {
    let mut imu = Mpu6050::new(Bus::new(), 0x68);
    let mut delay = Delay::default();
    imu.initialize(&mut delay).unwrap();
    let b = imu.into_inner();
    assert_eq!(delay.0, 200);
    assert_eq!(b.regs[0x6b], 1);
    assert_eq!(b.regs[0x1b], 0x18);
    assert_eq!(b.regs[0x1c], 0x10);
    assert_eq!(b.regs[0x19], 0);
    assert_eq!(b.regs[0x1a], 2);
    assert_eq!(b.regs[0x23], 0);
}
#[test]
fn refuses_wrong_part_nack_and_a_sensor_that_ignores_configuration() {
    let mut b = Bus::new();
    b.regs[0x75] = 0x70;
    assert_eq!(
        Mpu6050::new(b, 0x68).initialize(&mut Delay::default()),
        Err(Error::Identity(0x70))
    );
    let mut b = Bus::new();
    b.nack = true;
    assert_eq!(
        Mpu6050::new(b, 0x68).initialize(&mut Delay::default()),
        Err(Error::Bus(BusError))
    );
    let mut b = Bus::new();
    b.bad_readback = true;
    assert_eq!(
        Mpu6050::new(b, 0x68).initialize(&mut Delay::default()),
        Err(Error::Configuration(0x1b))
    );
}
#[test]
fn decodes_signed_burst_with_actual_eight_g_and_two_thousand_dps_scales() {
    let mut b = Bus::new();
    b.regs[0x3a] = 1;
    for (offset, value) in [
        (0, 4096i16),
        (2, -2048),
        (4, 8192),
        (6, 340),
        (8, 1640),
        (10, -820),
        (12, 0),
    ] {
        b.regs[0x3b + offset..0x3b + offset + 2].copy_from_slice(&value.to_be_bytes());
    }
    let s = Mpu6050::new(b, 0x68).sample().unwrap();
    assert!((s.accel.x - G).abs() < 0.0001);
    assert!((s.accel.y + G / 2.).abs() < 0.0001);
    assert!((s.accel.z - 2. * G).abs() < 0.0001);
    assert!((s.gyro.x - 100f32.to_radians()).abs() < 0.0001);
    assert!((s.gyro.y + 50f32.to_radians()).abs() < 0.0001);
    assert!((s.temperature_c - 37.53).abs() < 0.001);
}
#[test]
fn stale_and_saturated_samples_are_rejected() {
    assert!(matches!(
        Mpu6050::new(Bus::new(), 0x68).sample(),
        Err(Error::NotReady)
    ));
    let mut b = Bus::new();
    b.regs[0x3a] = 1;
    b.regs[0x3b..0x3d].copy_from_slice(&i16::MIN.to_be_bytes());
    assert!(matches!(
        Mpu6050::new(b, 0x68).sample(),
        Err(Error::Saturated)
    ));
}
