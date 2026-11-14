#![no_std]
#![no_main]
#![deny(clippy::mem_forget)]

use aerolab_esp32_devkitc::{BOARD, Bootstrap, CHIP, FLASH_BYTES, MOTOR_GPIOS, UART_BAUD};
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    time::Instant,
};
use esp_println::println;

// espflash and the ESP-IDF bootloader validate this descriptor when packaging.
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    // Static LOW, not 1000 us pulses. No ESC peripheral, arming path, network,
    // sensor driver or actuator command is enabled in milestone 1.
    // Keep these output handles alive for the application's entire lifetime.
    let inhibited = [
        Output::new(peripherals.GPIO25, Level::Low, OutputConfig::default()),
        Output::new(peripherals.GPIO26, Level::Low, OutputConfig::default()),
        Output::new(peripherals.GPIO27, Level::Low, OutputConfig::default()),
        Output::new(peripherals.GPIO14, Level::Low, OutputConfig::default()),
    ];

    println!(
        "AEROLAB firmware {} milestone=1 mode=boot_only",
        env!("CARGO_PKG_VERSION")
    );
    println!(
        "board={} chip={} configured_flash_bytes={} cpu_mhz=240 uart_baud={}",
        BOARD, CHIP, FLASH_BYTES, UART_BAUD
    );
    println!(
        "reserved_motor_gpios={:?} signal=static_low pwm=disabled",
        MOTOR_GPIOS
    );
    println!(
        "imu=unavailable battery=unavailable wifi=disabled stabilization=not_running flight_ready=false"
    );

    let mut bootstrap = Bootstrap::default();
    let delay = Delay::new();
    loop {
        // This reads GPIO output-register state, not measured pad voltage.
        // Physical reset/boot-ROM behavior still requires a bench measurement.
        let outputs_low = inhibited.iter().all(|pin| pin.is_set_low());
        let report = bootstrap.heartbeat(Instant::now().duration_since_epoch().as_millis());
        println!(
            "heartbeat={} uptime_ms={} core={:?} gpio_output_registers_low={} requested_pwm_us={:?} flight_ready=false",
            report.heartbeat,
            report.uptime_ms,
            report.controller_state,
            outputs_low,
            report.motor_requests.pwm_us
        );
        delay.delay_millis(1000);
    }
}
