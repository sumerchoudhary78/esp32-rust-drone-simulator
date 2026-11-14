use aerolab_esp32_devkitc::{BOARD, Bootstrap, FLASH_BYTES, MOTOR_GPIOS};
use flight_core::safety::FlightState;

#[test]
fn no_sensor_samples_means_no_calibration_or_arming_even_after_long_uptime() {
    let mut bootstrap = Bootstrap::default();
    for second in 0..3600 {
        let report = bootstrap.heartbeat(second * 1000);
        assert_eq!(report.controller_state, FlightState::Calibrating);
        assert_eq!(report.motor_requests.normalized, [0.; 4]);
        assert_eq!(report.motor_requests.pwm_us, [1000; 4]);
    }
}

#[test]
fn diagnostic_reports_follow_the_supplied_hardware_clock() {
    let mut bootstrap = Bootstrap::default();
    let first = bootstrap.heartbeat(75);
    let second = bootstrap.heartbeat(1079);
    assert_eq!((first.uptime_ms, first.heartbeat), (75, 1));
    assert_eq!((second.uptime_ms, second.heartbeat), (1079, 2));
    assert_eq!(second.motor_requests.normalized, [0.; 4]);
}

#[test]
fn target_pin_reservations_are_distinct_output_capable_non_flash_pins() {
    assert_eq!(BOARD, "ESP32-DevKitC-32E");
    assert_eq!(FLASH_BYTES, 4_194_304);
    for (index, pin) in MOTOR_GPIOS.iter().enumerate() {
        assert!(!MOTOR_GPIOS[..index].contains(pin));
        assert!(*pin < 34); // GPIO34..39 are input-only on classic ESP32.
        assert!(!(6..=11).contains(pin)); // Reserved for module SPI flash.
        assert!(![0, 2, 5, 12, 15].contains(pin)); // Chip strapping pins.
    }
}
