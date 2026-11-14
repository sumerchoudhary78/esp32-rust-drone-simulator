//! Simulation-only peripheral exerciser. It intentionally generates motor
//! pulses without flight arming or stabilization. The flash helper never
//! flashes this binary. Use it only in the virtual device workbench.
#![no_std]
#![no_main]
use esp_backtrace as _;
use esp_hal::{
    analog::adc::{Adc, AdcConfig, Attenuation},
    clock::CpuClock,
    delay::Delay,
    gpio::DriveMode,
    i2c::master::{Config, I2c},
    ledc::{
        LSGlobalClkSource, Ledc, LowSpeed,
        channel::{self, ChannelHW, ChannelIFace},
        timer::{self, TimerIFace},
    },
    time::Rate,
};
use esp_println::println;
esp_bootloader_esp_idf::esp_app_desc!();
#[esp_hal::main]
fn main() -> ! {
    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    let mut adc_config = AdcConfig::new();
    let mut battery_pin = adc_config.enable_pin(p.GPIO34, Attenuation::_11dB);
    let mut adc = Adc::new(p.ADC1, adc_config);
    println!("AEROLAB SIMULATION FIXTURE · not flight firmware · UART: 0=1000us 5=1500us 7=1700us");
    let mut i2c = I2c::new(
        p.I2C0,
        Config::default().with_frequency(Rate::from_khz(100)),
    )
    .unwrap()
    .with_sda(p.GPIO21)
    .with_scl(p.GPIO22);
    let mut id = [0u8];
    match i2c.write_read(0x68, &[0x75], &mut id) {
        Ok(()) => println!("MPU6050 WHO_AM_I=0x{:02x}", id[0]),
        Err(e) => println!("MPU6050 unavailable: {:?}", e),
    }
    let _ = i2c.write(0x68, &[0x6b, 0]);
    let mut ledc = Ledc::new(p.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
    let mut timer = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    timer
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty14Bit,
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_hz(50),
        })
        .unwrap();
    let mut channels = [
        ledc.channel(channel::Number::Channel0, p.GPIO25),
        ledc.channel(channel::Number::Channel1, p.GPIO26),
        ledc.channel(channel::Number::Channel2, p.GPIO27),
        ledc.channel(channel::Number::Channel3, p.GPIO14),
    ];
    for c in &mut channels {
        c.configure(channel::config::Config {
            timer: &timer,
            duty_pct: 5,
            drive_mode: DriveMode::PushPull,
        })
        .unwrap();
    }
    let delay = Delay::new();
    let mut pulse = 1000u32;
    let mut loops = 0u32;
    loop {
        // UART0 FIFO/status registers, classic ESP32. The emulator injects
        // bytes at this same UART boundary; no simulator-only function call.
        unsafe {
            while core::ptr::read_volatile(0x3ff4001c as *const u32) & 255 != 0 {
                match core::ptr::read_volatile(0x3ff40000 as *const u32) as u8 {
                    b'0' => pulse = 1000,
                    b'5' => pulse = 1500,
                    b'7' => pulse = 1700,
                    b'r' => {
                        let _ = i2c.write(0x68, &[0x1b, 0x18]);
                        let _ = i2c.write(0x68, &[0x1c, 0x18]);
                    }
                    _ => {}
                }
            }
        }
        for c in &channels {
            c.set_duty_hw(pulse * 16384 / 20000);
        }
        let mut data = [0u8; 14];
        let result = i2c.write_read(0x68, &[0x3b], &mut data);
        let battery_adc = loop {
            if let Ok(value) = adc.read_oneshot(&mut battery_pin) {
                break value;
            }
        };
        if loops % 10 == 0 {
            match result {
                Ok(()) => println!(
                    "sample az_raw={} gz_raw={} pwm_us={} battery_adc={}",
                    i16::from_be_bytes([data[4], data[5]]),
                    i16::from_be_bytes([data[12], data[13]]),
                    pulse,
                    battery_adc
                ),
                Err(e) => println!("IMU NACK/error {:?}", e),
            }
        }
        loops = loops.wrapping_add(1);
        delay.delay_millis(20);
    }
}
