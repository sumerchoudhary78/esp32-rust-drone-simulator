use drone_sim::{
    World,
    circuit::{Circuit, Kind, Pin},
    emulator::decode_outputs,
    firmware_upload::validate_bin,
};
use flight_core::{commands::Command, safety::FlightState};

fn disconnect(c: &mut Circuit, device: &str, pin: &str) {
    c.wires.retain(|w| {
        ![&w.from, &w.to]
            .iter()
            .any(|p| p.device == device && p.pin == pin)
    });
}
#[test]
fn reference_has_all_devices_and_resolves_signal_and_power_nets() {
    let c = Circuit::preset();
    let v = c.validate();
    assert!(v.valid, "{:?}", v.errors);
    assert!(v.flight_connected);
    assert_eq!(c.devices.len(), 13);
    assert_eq!(
        v.wiring.motor_gpios,
        [Some(25), Some(26), Some(27), Some(14)]
    );
    assert_eq!(v.wiring.sda_gpio, Some(21));
    assert_eq!(v.wiring.scl_gpio, Some(22));
    assert_eq!(v.wiring.adc_gpio, Some(34));
    assert_eq!(c.devices.iter().filter(|d| d.kind == Kind::Esc).count(), 4);
}
#[test]
fn empty_and_partial_assemblies_are_valid_but_do_not_enable_flight() {
    let v = Circuit::empty().validate();
    assert!(v.valid);
    assert!(!v.flight_connected);
    assert!(!v.wiring.esp_powered);
    for (device, pin) in [
        ("imu", "VCC"),
        ("imu", "SDA"),
        ("imu", "GND"),
        ("esp", "5V"),
        ("esc1", "GND"),
        ("motor2", "B"),
        ("divider", "OUT"),
    ] {
        let mut c = Circuit::preset();
        disconnect(&mut c, device, pin);
        let v = c.validate();
        assert!(v.valid, "{device}.{pin} {:?}", v.errors);
        assert!(!v.flight_connected, "{device}.{pin}");
    }
}
#[test]
fn power_shorts_and_gpio_and_motor_phase_shorts_are_rejected() {
    for (a, b) in [
        ("battery.+", "battery.-"),
        ("battery.+", "esp.5V"),
        ("esp.3V3", "esp.GPIO25"),
        ("esp.GPIO25", "esp.GPIO26"),
        ("esc1.A", "esc1.B"),
        ("esc1.A", "battery.+"),
        ("battery.+", "imu.VCC"),
    ] {
        let mut c = Circuit::preset();
        c.link(a, b);
        let v = c.validate();
        assert!(!v.valid, "{a} {b}");
        assert!(!v.flight_connected);
    }
}
#[test]
fn input_only_pin_cannot_drive_esc_and_alternate_i2c_address_is_resolved() {
    let mut c = Circuit::preset();
    disconnect(&mut c, "esc1", "SIGNAL");
    c.link("esp.GPIO35", "esc1.SIGNAL");
    let v = c.validate();
    assert!(v.valid);
    assert_eq!(v.wiring.motor_gpios[0], None);
    assert!(!v.wiring.motors_connected[0]);
    disconnect(&mut c, "imu", "AD0");
    c.link("esp.3V3", "imu.AD0");
    assert_eq!(c.validate().wiring.imu_address, 0x69);
}
#[test]
fn net_direction_and_board_ground_aliases_do_not_change_connectivity() {
    let mut c = Circuit::preset();
    for w in &mut c.wires {
        std::mem::swap(&mut w.from, &mut w.to);
        for p in [&mut w.from, &mut w.to] {
            if p.device == "esp" && p.pin == "GND1" {
                p.pin = "GND3".into();
            }
        }
    }
    assert!(c.validate().flight_connected);
}
#[test]
fn malformed_circuits_fail_validation_without_panics() {
    let mut c = Circuit::preset();
    c.devices.push(c.devices[0].clone());
    assert!(!c.validate().valid);
    let mut c = Circuit::preset();
    c.wires[0].to = Pin {
        device: "missing".into(),
        pin: "GPIO25".into(),
    };
    assert!(!c.validate().valid);
    let mut c = Circuit::preset();
    c.devices[0].x = f32::NAN;
    assert!(!c.validate().valid);
    let mut c = Circuit::preset();
    c.devices
        .iter_mut()
        .find(|d| d.kind == Kind::Esc)
        .unwrap()
        .slot = Some(5);
    assert!(!c.validate().valid);
    let mut c = Circuit::preset();
    c.wires = vec![c.wires[0].clone(); 129];
    assert!(!c.validate().valid);
}
#[test]
fn pulse_decoder_gates_by_wiring_and_rejects_non_pwm_signals() {
    let mut w = Circuit::preset().validate().wiring;
    let mut p = vec![0.; 40];
    for n in [25, 26, 27, 14] {
        p[n] = 1500.;
    }
    assert_eq!(decode_outputs(&w, &p).normalized, [0.5; 4]);
    w.motors_connected[1] = false;
    p[27] = 2500.;
    p[14] = f32::NAN;
    let out = decode_outputs(&w, &p);
    assert_eq!(out.pwm_us, [1500, 1000, 1000, 1000]);
    p[25] = 999.756;
    assert_eq!(decode_outputs(&w, &p).normalized, [0.; 4]);
}
#[test]
fn changing_circuit_disarms_and_reset_preserves_the_applied_assembly() {
    let mut w = World::default();
    w.connect(1);
    for _ in 0..520 {
        w.step();
    }
    w.receive(Command {
        sequence: 1,
        ..Command::default()
    })
    .unwrap();
    for _ in 0..20 {
        w.step();
    }
    w.receive(Command {
        sequence: 2,
        arm: true,
        ..Command::default()
    })
    .unwrap();
    for _ in 0..20 {
        w.step();
    }
    assert_eq!(w.core.state, FlightState::Armed);
    let mut c = Circuit::preset();
    disconnect(&mut c, "imu", "VCC");
    w.apply_circuit(c).unwrap();
    assert_ne!(w.core.state, FlightState::Armed);
    for _ in 0..600 {
        w.step();
    }
    assert_eq!(w.core.calibration_samples, 0);
    assert_eq!(w.snapshot().motors.normalized, [0.; 4]);
    w.action(drone_sim::Action::Reset).unwrap();
    assert!(!w.validation.wiring.imu_powered);
    assert_eq!(w.controller_id, Some(1));
    assert!(
        w.receive(Command {
            sequence: 3,
            arm: true,
            ..Command::default()
        })
        .is_err()
    );
}
#[test]
fn incompatible_truncated_and_non_esp32_uploads_are_rejected() {
    let mut image = vec![0; 48];
    image[0] = 0xe9;
    image[1] = 1;
    image[28..32].copy_from_slice(&4u32.to_le_bytes());
    image[47] = 0xef;
    assert!(validate_bin(&image).is_ok());
    image[12] = 5;
    assert!(validate_bin(&image).is_err());
    image[12] = 0;
    image[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(validate_bin(&image).is_err());
    for len in 0..48 {
        assert!(validate_bin(&image[..len]).is_err());
    }
    assert!(validate_bin(b"arbitrary text is not an ESP32 app image").is_err());
}
