//! Real GPIO/I²C/ADC/LEDC flight-controller adapter. Default output is capped
//! for a propellers-removed bench. Physical qualification is a separate result.
#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]
use aerolab_esp32_devkitc::{
    commands::{Receiver, UartFrame},
    controller::{Controller, PERIOD_US},
    imu::Mpu6050,
    mailbox::Mailbox,
    profile,
};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use esp_backtrace as _;
use esp_hal::ram;
use esp_hal::{
    analog::adc::{Adc, AdcConfig, Attenuation},
    clock::CpuClock,
    delay::Delay,
    gpio::{DriveMode, Input, InputConfig, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c, SoftwareTimeout},
    ledc::{
        LSGlobalClkSource, Ledc, LowSpeed,
        channel::{self, ChannelHW, ChannelIFace},
        timer::{self, TimerIFace},
    },
    system::{AppCoreGuard, CpuControl, Stack},
    time::{Duration, Instant, Rate},
    timer::{
        PeriodicTimer,
        timg::{MwdtStage, TimerGroup},
    },
};
use esp_println::println;
use flight_core::safety::{FlightState, StopReason};
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();
static STACK: StaticCell<Stack<32768>> = StaticCell::new();
static COMMANDS: Mailbox = Mailbox::new();
static HEARTBEAT: AtomicU32 = AtomicU32::new(0);
static WATCH_TICKS: AtomicU32 = AtomicU32::new(0);
static WATCH_SEEN: AtomicU32 = AtomicU32::new(0);
static WATCH_STALE: AtomicU32 = AtomicU32::new(0);
static WATCH_TRIPPED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "bench-fault-injection")]
static STALL: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "bench-fault-injection")]
static MASK_INTERRUPTS: AtomicBool = AtomicBool::new(false);
static STATE: AtomicU32 = AtomicU32::new(0);
static CALIBRATION: AtomicU32 = AtomicU32::new(0);
static ADC_RAW: AtomicU32 = AtomicU32::new(0);
static VOLTAGE: AtomicU32 = AtomicU32::new(0);
static GATE: AtomicBool = AtomicBool::new(false);
static IMU_ERRORS: AtomicU32 = AtomicU32::new(0);
static MAX_EXEC_US: AtomicU32 = AtomicU32::new(0);
static OVERRUNS: AtomicU32 = AtomicU32::new(0);
static REJECTED: AtomicU32 = AtomicU32::new(0);
static MOTION: [AtomicU32; 6] = [const { AtomicU32::new(0) }; 6];
static MOTORS: [AtomicU32; 4] = [const { AtomicU32::new(1000) }; 4];
static REASON: AtomicU32 = AtomicU32::new(0);
const MOTOR_MASK: u32 = (1 << 25) | (1 << 26) | (1 << 27) | (1 << 14);

fn now_us() -> u32 {
    Instant::now().duration_since_epoch().as_micros() as u32
}

// IRAM-only, no formatting, locks, allocation, I²C or network activity. This
// handler runs from a separate TIMG1 timer even if the flight loop is stuck.
#[esp_hal::handler(priority=esp_hal::interrupt::Priority::Priority3)]
#[ram]
fn motor_watchdog() {
    WATCH_TICKS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        core::ptr::write_volatile(0x3ff600a4 as *mut u32, 1);
        // TIMG clears ALARM_EN at every match, including autoreload mode.
        // Re-arm it on each independent watchdog tick.
        let config = 0x3ff60000 as *mut u32;
        core::ptr::write_volatile(config, core::ptr::read_volatile(config) | (1 << 10));
    }
    let count = HEARTBEAT.load(Ordering::Relaxed);
    let old = WATCH_SEEN.swap(count, Ordering::Relaxed);
    let stale = if count == old {
        WATCH_STALE.load(Ordering::Relaxed) + 1
    } else {
        0
    };
    WATCH_STALE.store(stale, Ordering::Relaxed);
    if stale >= 5 {
        WATCH_TRIPPED.store(true, Ordering::SeqCst);
        GATE.store(false, Ordering::Relaxed);
        STATE.store(3, Ordering::Relaxed);
        REASON.store(StopReason::TimingOverrun as u32 + 1, Ordering::Relaxed);
        unsafe {
            core::ptr::write_volatile(0x3ff44018 as *mut u32, 1); // GPIO32 gate LOW
            core::ptr::write_volatile(0x3ff44028 as *mut u32, MOTOR_MASK); // pads disabled
            core::ptr::write_volatile(0x3ff4400c as *mut u32, MOTOR_MASK);
        }
    }
}

struct Comms {
    receiver: Receiver,
    session: u64,
    #[cfg(feature = "bench-wifi")]
    wifi: esp_hal::peripherals::WIFI<'static>,
    #[cfg(feature = "bench-wifi")]
    timer: esp_hal::peripherals::TIMG0<'static>,
    #[cfg(feature = "bench-wifi")]
    interrupt: esp_hal::peripherals::FROM_CPU_INTR0<'static>,
}
#[cfg(not(feature = "bench-wifi"))]
#[esp_hal::main]
fn main() -> ! {
    let (_flight, comms) = setup();
    serial(comms.receiver, comms.session)
}
#[cfg(feature = "bench-wifi")]
#[esp_hal::main]
async fn main(spawner: embassy_executor::Spawner) -> ! {
    let (_flight, comms) = setup();
    network::run(spawner, comms).await
}
fn setup() -> (AppCoreGuard<'static>, Comms) {
    let mut p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    // An external pull-down and 4-channel AND gate are required for reset/boot
    // inhibition. This software output is LOW before the second core starts.
    let gate = Output::new(p.GPIO32, Level::Low, OutputConfig::default());
    let session = if profile::SIMULATED {
        0x1122334455667788u64
    } else {
        let source = esp_hal::rng::TrngSource::new(p.RNG, p.ADC1.reborrow());
        Delay::new().delay_millis(20);
        let random = esp_hal::rng::Trng::try_new().unwrap();
        let mut bytes = [0; 8];
        random.read(&mut bytes);
        let _rng = random.downgrade();
        drop(source);
        u64::from_le_bytes(bytes)
    };
    println!(
        "AEROLAB flight-controller bench board=ESP32-DevKitC-32E profile={} flight_qualified=false",
        if profile::SIMULATED {
            "emulated_not_for_flash"
        } else {
            "physical_unqualified"
        }
    );
    println!(
        "session={:016x} command=AERO2_hmac_sha256 loop_hz=500 esc_hz={} max_pwm_us={} adc_calibrated={} arm_gpio=13 gate_gpio=32",
        session,
        profile::ESC_HZ,
        profile::MAX_PWM_US,
        profile::CALIBRATION.is_some()
    );
    let mut cpu = CpuControl::new(p.CPU_CTRL);
    let flight = cpu
        .start_app_core(STACK.init(Stack::new()), move || {
            let mut gate = gate;
            let mut marker = Output::new(p.GPIO33, Level::Low, OutputConfig::default());
            let arm = Input::new(p.GPIO13, InputConfig::default().with_pull(Pull::Down));
            let i2c = I2c::new(
                p.I2C0,
                I2cConfig::default()
                    .with_frequency(Rate::from_khz(400))
                    .with_software_timeout(SoftwareTimeout::Transaction(Duration::from_micros(
                        750,
                    ))),
            )
            .unwrap()
            .with_sda(p.GPIO21)
            .with_scl(p.GPIO22);
            let mut imu = Mpu6050::new(i2c, 0x68);
            let mut delay = Delay::new();
            let mut imu_ready = imu.initialize(&mut delay).is_ok();
            let mut adc_config = AdcConfig::new();
            let mut battery_pin = adc_config.enable_pin(p.GPIO34, Attenuation::_11dB);
            let mut adc = Adc::new(p.ADC1, adc_config);
            let mut ledc = Ledc::new(p.LEDC);
            ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
            let mut timer = ledc.timer::<LowSpeed>(timer::Number::Timer0);
            timer
                .configure(timer::config::Config {
                    duty: timer::config::Duty::Duty16Bit,
                    clock_source: timer::LSClockSource::APBClk,
                    frequency: Rate::from_hz(profile::ESC_HZ),
                })
                .unwrap();
            let mut motors = [
                ledc.channel(channel::Number::Channel0, p.GPIO25),
                ledc.channel(channel::Number::Channel1, p.GPIO26),
                ledc.channel(channel::Number::Channel2, p.GPIO27),
                ledc.channel(channel::Number::Channel3, p.GPIO14),
            ];
            for motor in &mut motors {
                motor
                    .configure(channel::config::Config {
                        timer: &timer,
                        duty_pct: 0,
                        drive_mode: DriveMode::PushPull,
                    })
                    .unwrap();
                motor.set_duty_hw((1000u64 * profile::ESC_HZ as u64 * 65536 / 1_000_000) as u32);
            }
            let mut timers = TimerGroup::new(p.TIMG1);
            let mut watchdog = PeriodicTimer::new(timers.timer0);
            watchdog.set_interrupt_handler(motor_watchdog);
            watchdog.start(Duration::from_micros(2000)).unwrap();
            watchdog.listen();
            // MWDT uses a hardware counter, fed only after a completed control cycle.
            // It resets the system even if the timer ISR cannot execute. Its reset
            // response requires an actual scope test; the emulator does not prove it.
            timers
                .wdt
                .set_timeout(MwdtStage::Stage0, Duration::from_millis(40));
            timers.wdt.enable();
            timers.wdt.feed();
            // Core 1 enters through the ROM handoff, not the #[main] runtime.
            // Vectors and the watchdog handler are now installed; permit their
            // interrupt priority explicitly instead of inheriting the ROM PS.
            unsafe {
                core::arch::asm!("rsil {previous}, 0", previous=out(reg) _, options(nostack));
            }
            let mut controller = Controller::new(profile::CALIBRATION, profile::MAX_PWM_US);
            let mut epoch = 0;
            let mut due = now_us();
            let mut retry_at = due;
            let mut sensor_errors = 0;
            loop {
                while (now_us().wrapping_sub(due) as i32) < 0 {
                    core::hint::spin_loop();
                }
                let begin = now_us();
                marker.set_high();
                if begin.wrapping_sub(due) > 500 {
                    controller.cut(StopReason::TimingOverrun);
                    OVERRUNS.fetch_add(1, Ordering::Relaxed);
                }
                due = due.wrapping_add(PERIOD_US);
                if begin.wrapping_sub(due) < 0x80000000 {
                    due = begin.wrapping_add(PERIOD_US);
                }
                if WATCH_TRIPPED.load(Ordering::SeqCst) {
                    controller.cut(StopReason::TimingOverrun);
                }
                if COMMANDS.take_emergency() {
                    controller.cut(StopReason::EmergencyStop);
                }
                if let Some(packet) = COMMANDS.read_new(&mut epoch) {
                    controller.command(packet, begin, arm.is_high());
                }
                #[cfg(feature = "bench-fault-injection")]
                if STALL.load(Ordering::Relaxed) {
                    if MASK_INTERRUPTS.load(Ordering::Relaxed) {
                        xtensa_lx::interrupt::disable();
                    }
                    loop {
                        core::hint::spin_loop();
                    }
                }
                // Sensor recovery is attempted only with motor gate LOW. Its delays
                // run before the watchdog is re-enabled, while all outputs are cut.
                if !imu_ready
                    && controller.core.state != FlightState::Armed
                    && (begin.wrapping_sub(retry_at) as i32) >= 0
                {
                    gate.set_low();
                    GATE.store(false, Ordering::Relaxed);
                    timers.wdt.disable();
                    watchdog.unlisten();
                    imu_ready = imu.initialize(&mut delay).is_ok();
                    retry_at = now_us().wrapping_add(1_000_000);
                    controller.reset_calibration();
                    sensor_errors = 0;
                    WATCH_STALE.store(0, Ordering::Relaxed);
                    WATCH_SEEN.store(HEARTBEAT.load(Ordering::Relaxed), Ordering::Relaxed);
                    watchdog.start(Duration::from_micros(2000)).unwrap();
                    watchdog.clear_interrupt();
                    timers.wdt.enable();
                    timers.wdt.feed();
                    watchdog.listen();
                    due = now_us().wrapping_add(PERIOD_US);
                    marker.set_low();
                    continue;
                }
                let sample = if imu_ready {
                    match imu.sample() {
                        Ok(s) => {
                            sensor_errors = 0;
                            Some(s)
                        }
                        Err(_) => {
                            IMU_ERRORS.fetch_add(1, Ordering::Relaxed);
                            sensor_errors += 1;
                            if sensor_errors >= 3 {
                                imu_ready = false;
                            }
                            None
                        }
                    }
                } else {
                    None
                };
                let adc_start = now_us();
                let raw = loop {
                    if let Ok(raw) = adc.read_oneshot(&mut battery_pin) {
                        break Some(raw);
                    }
                    if now_us().wrapping_sub(adc_start) > 150 {
                        break None;
                    }
                };
                ADC_RAW.store(raw.unwrap_or(0) as u32, Ordering::Relaxed);
                let out = controller.step(sample, raw, arm.is_high(), begin);
                let elapsed = now_us().wrapping_sub(begin);
                MAX_EXEC_US.fetch_max(elapsed, Ordering::Relaxed);
                if elapsed >= 1500 {
                    controller.cut(StopReason::TimingOverrun);
                    OVERRUNS.fetch_add(1, Ordering::Relaxed);
                }
                let enabled =
                    controller.gate && elapsed < 1500 && !WATCH_TRIPPED.load(Ordering::SeqCst);
                if !enabled {
                    gate.set_low();
                }
                for (motor, pulse) in motors.iter().zip(out.pwm_us) {
                    let pulse = if enabled { pulse } else { 1000 };
                    motor.set_duty_hw(
                        (pulse as u64 * profile::ESC_HZ as u64 * 65536 / 1_000_000) as u32,
                    );
                }
                if enabled {
                    gate.set_high();
                    if WATCH_TRIPPED.load(Ordering::SeqCst) {
                        gate.set_low();
                    }
                }
                GATE.store(
                    enabled && !WATCH_TRIPPED.load(Ordering::SeqCst),
                    Ordering::Relaxed,
                );
                STATE.store(
                    match controller.core.state {
                        FlightState::Calibrating => 0,
                        FlightState::Disarmed => 1,
                        FlightState::Armed => 2,
                        FlightState::Failsafe => 3,
                    },
                    Ordering::Relaxed,
                );
                CALIBRATION.store(
                    controller.stationary + controller.core.calibration_samples,
                    Ordering::Relaxed,
                );
                VOLTAGE.store(
                    controller.voltage.unwrap_or(0.).to_bits(),
                    Ordering::Relaxed,
                );
                for (i, v) in [
                    controller.core.attitude.x,
                    controller.core.attitude.y,
                    controller.core.attitude.z,
                    controller.core.rates.x,
                    controller.core.rates.y,
                    controller.core.rates.z,
                ]
                .iter()
                .enumerate()
                {
                    MOTION[i].store(v.to_bits(), Ordering::Relaxed);
                }
                for (i, pulse) in out.pwm_us.iter().enumerate() {
                    MOTORS[i].store(
                        if enabled { *pulse as u32 } else { 1000 },
                        Ordering::Relaxed,
                    );
                }
                REASON.store(
                    controller.core.reason.map_or(0, |r| r as u32 + 1),
                    Ordering::Relaxed,
                );
                HEARTBEAT.fetch_add(1, Ordering::Relaxed);
                timers.wdt.feed();
                marker.set_low();
            }
        })
        .unwrap();
    (
        flight,
        Comms {
            receiver: Receiver::new(profile::COMMAND_KEY, session),
            session,
            #[cfg(feature = "bench-wifi")]
            wifi: p.WIFI,
            #[cfg(feature = "bench-wifi")]
            timer: p.TIMG0,
            #[cfg(feature = "bench-wifi")]
            interrupt: p.FROM_CPU_INTR0,
        },
    )
}

fn receive_uart(receiver: &mut Receiver, frame: &mut UartFrame) {
    for _ in 0..128 {
        let byte = unsafe {
            if core::ptr::read_volatile(0x3ff4001c as *const u32) & 255 == 0 {
                break;
            }
            core::ptr::read_volatile(0x3ff40000 as *const u32) as u8
        };
        #[cfg(feature = "bench-fault-injection")]
        if byte == b'!' || byte == b'~' {
            MASK_INTERRUPTS.store(byte == b'~', Ordering::Relaxed);
            STALL.store(true, Ordering::Relaxed);
        }
        if let Some(bytes) = frame.push(byte) {
            match receiver.receive(&bytes, now_us()) {
                Ok(p) => COMMANDS.publish(p),
                Err(_) => {
                    REJECTED.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}
#[cfg(not(feature = "bench-wifi"))]
fn serial(mut receiver: Receiver, session: u64) -> ! {
    let mut frame = UartFrame::default();
    let mut next = now_us();
    loop {
        receive_uart(&mut receiver, &mut frame);
        if (now_us().wrapping_sub(next) as i32) >= 0 {
            status(session, receiver.next_sequence());
            next = now_us().wrapping_add(100_000);
        }
    }
}
fn status(session: u64, next_sequence: u32) {
    println!(
        "bench session={:016x} next={} state={} calibration={} adc_raw={} voltage={:.3} gate={} imu_errors={} max_exec_us={} overruns={} watchdog={} rejected={} roll={:.4} pitch={:.4} yaw={:.4} gx={:.4} gy={:.4} gz={:.4} pwm={},{},{},{} reason={} watchdog_ticks={}",
        session,
        next_sequence,
        STATE.load(Ordering::Relaxed),
        CALIBRATION.load(Ordering::Relaxed),
        ADC_RAW.load(Ordering::Relaxed),
        f32::from_bits(VOLTAGE.load(Ordering::Relaxed)),
        GATE.load(Ordering::Relaxed),
        IMU_ERRORS.load(Ordering::Relaxed),
        MAX_EXEC_US.load(Ordering::Relaxed),
        OVERRUNS.load(Ordering::Relaxed),
        WATCH_TRIPPED.load(Ordering::Relaxed),
        REJECTED.load(Ordering::Relaxed),
        f32::from_bits(MOTION[0].load(Ordering::Relaxed)),
        f32::from_bits(MOTION[1].load(Ordering::Relaxed)),
        f32::from_bits(MOTION[2].load(Ordering::Relaxed)),
        f32::from_bits(MOTION[3].load(Ordering::Relaxed)),
        f32::from_bits(MOTION[4].load(Ordering::Relaxed)),
        f32::from_bits(MOTION[5].load(Ordering::Relaxed)),
        MOTORS[0].load(Ordering::Relaxed),
        MOTORS[1].load(Ordering::Relaxed),
        MOTORS[2].load(Ordering::Relaxed),
        MOTORS[3].load(Ordering::Relaxed),
        REASON.load(Ordering::Relaxed),
        WATCH_TICKS.load(Ordering::Relaxed)
    );
}

#[cfg(feature = "bench-wifi")]
#[path = "network.rs"]
mod network;
