use aerolab_esp32_devkitc::{
    battery::{Battery, Calibration},
    commands::{self, Accepted, Receiver, UartFrame},
    controller::{Controller, STATIONARY_SAMPLES},
    mailbox::Mailbox,
};
use flight_core::{
    commands::Command,
    imu::{G, ImuSample},
    math::Vec3,
    safety::{FlightState, StopReason},
};
const KEY: [u8; 32] = [0x42; 32];
const SESSION: u64 = 0x1122334455667788;
fn calibration() -> Calibration {
    Calibration {
        raw_low: 2400,
        volts_low: 9.,
        raw_high: 3360,
        volts_high: 12.6,
    }
}
fn sample() -> ImuSample {
    ImuSample {
        accel: Vec3::new(0., 0., G),
        gyro: Vec3::ZERO,
        temperature_c: 25.,
    }
}
fn ready() -> (Controller, u32) {
    let mut c = Controller::new(Some(calibration()), 1150);
    let mut now = 0;
    for _ in 0..2100 {
        c.step(Some(sample()), Some(3300), false, now);
        now += 2000;
    }
    assert_eq!(c.core.state, FlightState::Disarmed);
    (c, now)
}
fn command(c: &mut Controller, sequence: u64, arm: bool, throttle: f32, switch: bool, now: u32) {
    c.command(
        Accepted {
            command: Command {
                sequence,
                arm,
                throttle,
                ..Command::default()
            },
            received_us: now,
            emergency: false,
        },
        now,
        switch,
    );
}
fn arm(c: &mut Controller, now: u32) {
    command(c, 1, false, 0., false, now);
    command(c, 2, true, 0., true, now);
    c.step(Some(sample()), Some(3300), true, now);
    assert_eq!(c.core.state, FlightState::Armed);
    assert!(c.gate);
}

#[test]
fn authenticated_packets_validate_every_boundary_and_replay_counter() {
    let c = Command {
        arm: true,
        throttle: 0.2,
        roll: -0.4,
        pitch: 0.3,
        yaw: 1.,
        ..Command::default()
    };
    let bytes = commands::encode(&KEY, SESSION, 5, c, false);
    let vector = "4145524f02013c00887766554433221105000000c80070fe2c01e80325d96b6961c07ed0131f2a6fd92374196111bd58d876103d06627c6b32958c64";
    assert_eq!(
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        vector
    );
    let mut rx = Receiver::new(KEY, SESSION);
    let p = rx.receive(&bytes, 100).unwrap();
    assert_eq!(p.received_us, 100);
    assert_eq!(p.command.throttle, 0.2);
    assert_eq!(p.command.roll, -0.4);
    assert_eq!(
        rx.receive(&bytes, 200).unwrap_err(),
        commands::Error::Replay
    );
    for i in 0..60 {
        let mut corrupt = bytes;
        corrupt[i] ^= 0x40;
        assert!(
            Receiver::new(KEY, SESSION).receive(&corrupt, 0).is_err(),
            "byte {i}"
        );
    }
    assert_eq!(
        Receiver::new(KEY, 1).receive(&bytes, 0).unwrap_err(),
        commands::Error::Session
    );
    let bad = commands::encode(
        &KEY,
        SESSION,
        6,
        Command {
            throttle: 1.1,
            ..Command::default()
        },
        false,
    );
    assert_eq!(rx.receive(&bad, 0).unwrap_err(), commands::Error::Range);
    let valid = commands::encode(&KEY, SESSION, 6, Command::default(), false);
    assert!(rx.receive(&valid, 0).is_ok());
}
#[test]
fn packet_counter_wrap_is_accepted_without_allowing_old_data() {
    let mut r = Receiver::new(KEY, SESSION);
    for seq in [u32::MAX - 1, u32::MAX, 0, 1] {
        assert!(
            r.receive(
                &commands::encode(&KEY, SESSION, seq, Command::default(), false),
                0
            )
            .is_ok()
        );
    }
    assert!(
        r.receive(
            &commands::encode(&KEY, SESSION, u32::MAX, Command::default(), false),
            0
        )
        .is_err()
    );
}
#[test]
fn uart_parser_is_bounded_and_recovers_after_bad_or_partial_frames() {
    let p = commands::encode(&KEY, SESSION, 1, Command::default(), false);
    let hex = format!(
        "@{}\n",
        p.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
    let mut rx = UartFrame::default();
    for b in b"@xxx\n@"
        .iter()
        .copied()
        .chain(core::iter::repeat_n(b'0', 300))
        .chain(*b"\n")
    {
        assert!(rx.push(b).is_none());
    }
    let parsed = hex.bytes().find_map(|b| rx.push(b)).unwrap();
    assert_eq!(parsed, p);
}
#[test]
fn battery_requires_calibration_and_bad_or_critical_readings_act_immediately() {
    assert!(
        !Calibration {
            raw_low: 3000,
            raw_high: 2000,
            ..calibration()
        }
        .valid()
    );
    assert_eq!(calibration().voltage(3300), Some(12.375));
    let mut b = Battery::new(None);
    for _ in 0..100 {
        assert_eq!(b.update(Some(3300)), None);
    }
    let mut b = Battery::new(Some(calibration()));
    for _ in 0..9 {
        assert_eq!(b.update(Some(3300)), None);
    }
    assert!(b.update(Some(3300)).unwrap() > 12.);
    assert!(b.update(Some(2600)).unwrap() < 10.);
    assert_eq!(b.update(Some(4095)), None);
    assert_eq!(b.update(None), None);
}
#[test]
fn physical_switch_disarm_sequence_and_calibration_gate_arming() {
    let (mut c, now) = ready();
    command(&mut c, 1, true, 0., true, now);
    c.step(Some(sample()), Some(3300), true, now);
    assert!(!c.gate);
    command(&mut c, 2, false, 0., true, now);
    command(&mut c, 3, true, 0., true, now);
    c.step(Some(sample()), Some(3300), true, now);
    assert!(!c.gate);
    command(&mut c, 4, false, 0., false, now);
    command(&mut c, 5, true, 0., true, now);
    let out = c.step(Some(sample()), Some(3300), true, now);
    assert!(c.gate);
    assert!(out.pwm_us.iter().all(|p| *p <= 1150));
}
#[test]
fn uncalibrated_voltage_and_switch_held_high_at_boot_never_arm() {
    let mut c = Controller::new(None, 1150);
    for i in 0..2200 {
        command(&mut c, i + 1, true, 0., true, (i * 2000) as u32);
        c.step(Some(sample()), Some(3300), true, (i * 2000) as u32);
        assert!(!c.gate);
    }
    assert!(!c.switch_low_seen);
    assert!(c.voltage.is_none());
}
#[test]
fn motion_resets_stationary_calibration_and_early_arm_does_not_latch_on_ready() {
    let mut c = Controller::new(Some(calibration()), 1150);
    for i in 0..100 {
        c.step(Some(sample()), Some(3300), false, i * 2000);
    }
    assert_eq!(c.stationary, 100);
    c.step(
        Some(ImuSample {
            gyro: Vec3::new(1., 0., 0.),
            ..sample()
        }),
        Some(3300),
        false,
        200000,
    );
    assert_eq!(c.stationary, 0);
    for i in 0..STATIONARY_SAMPLES + 600 {
        command(&mut c, i as u64 + 1, true, 0., true, 202000 + i * 2000);
        c.step(Some(sample()), Some(3300), true, 202000 + i * 2000);
    }
    assert!(!c.gate);
}
#[test]
fn bench_caps_outputs_even_for_full_throttle_commands() {
    let (mut c, now) = ready();
    arm(&mut c, now);
    command(&mut c, 3, true, 1., true, now + 2000);
    let out = c.step(Some(sample()), Some(3300), true, now + 2000);
    assert_eq!(out.pwm_us, [1150; 4]);
}
#[test]
fn expired_commands_cut_outputs_without_auto_rearm_and_drop_stale_mailbox() {
    let (mut c, now) = ready();
    arm(&mut c, now);
    c.step(Some(sample()), Some(3300), true, now + 250000);
    assert!(!c.gate);
    assert_eq!(c.core.reason, Some(StopReason::LinkTimeout));
    command(&mut c, 3, true, 0., true, now + 252000);
    c.step(Some(sample()), Some(3300), true, now + 252000);
    assert!(!c.gate);
    let old = Accepted {
        command: Command::default(),
        received_us: now,
        emergency: false,
    };
    c.command(old, now + 300000, false);
    assert_eq!(c.core.state, FlightState::Failsafe);
}
#[test]
fn sensor_loss_battery_loss_physical_disarm_and_emergency_cut_the_gate() {
    for fault in 0..4 {
        let (mut c, now) = ready();
        arm(&mut c, now);
        c.step(
            if fault == 0 { None } else { Some(sample()) },
            if fault == 1 { None } else { Some(3300) },
            fault != 2,
            now + 2000,
        );
        if fault != 3 {
            assert!(!c.gate, "fault {fault}");
        }
        if fault == 3 {
            c.command(
                Accepted {
                    command: Command::default(),
                    received_us: now + 2000,
                    emergency: true,
                },
                now + 2000,
                true,
            );
            assert!(!c.gate);
        }
    }
}
#[test]
fn latest_mailbox_and_latched_emergency_cannot_lose_stop_to_a_later_command() {
    let m = Mailbox::new();
    let mut epoch = 0;
    let p = Accepted {
        command: Command::default(),
        received_us: 0,
        emergency: true,
    };
    m.publish(p);
    m.publish(Accepted {
        emergency: false,
        received_us: 1,
        ..p
    });
    assert!(m.take_emergency());
    assert!(!m.take_emergency());
    assert_eq!(m.read_new(&mut epoch).unwrap().received_us, 1);
    assert!(m.read_new(&mut epoch).is_none());
}
#[test]
fn cross_core_mailbox_has_no_torn_fields_under_contention() {
    use std::sync::Arc;
    let m = Arc::new(Mailbox::new());
    let tx = m.clone();
    let worker = std::thread::spawn(move || {
        for n in 1..50000 {
            tx.publish(Accepted {
                command: Command {
                    sequence: n,
                    throttle: n as f32,
                    ..Command::default()
                },
                received_us: n as u32,
                emergency: false,
            });
        }
    });
    let mut epoch = 0;
    while !worker.is_finished() {
        if let Some(p) = m.read_new(&mut epoch) {
            assert_eq!(p.command.sequence as u32, p.received_us);
            assert_eq!(p.command.throttle, p.received_us as f32);
        }
    }
    worker.join().unwrap();
}

#[test]
fn critical_voltage_tilt_and_recovered_sensor_require_a_new_arming_sequence() {
    let (mut c, now) = ready();
    arm(&mut c, now);
    c.step(Some(sample()), Some(2600), true, now + 2000);
    assert!(!c.gate);
    assert_eq!(c.core.reason, Some(StopReason::LowBattery));
    let (mut c, now) = ready();
    arm(&mut c, now);
    // The estimator is the owning source of attitude. Rotate a real sample
    // through it long enough to cross the configured tilt limit.
    for i in 1..200 {
        command(&mut c, i + 2, true, 0., true, now + i as u32 * 2000);
        c.step(
            Some(ImuSample {
                accel: Vec3::new(0., G, 0.),
                gyro: Vec3::new(4., 0., 0.),
                temperature_c: 25.,
            }),
            Some(3300),
            true,
            now + i as u32 * 2000,
        );
        if !c.gate {
            break;
        }
    }
    assert!(!c.gate);
    assert_eq!(c.core.reason, Some(StopReason::ExcessiveTilt));
    c.reset_calibration();
    assert_eq!(c.stationary, 0);
    assert_eq!(c.core.state, FlightState::Calibrating);
    assert!(!c.gate);
}
#[test]
fn arm_during_bias_calibration_cannot_become_a_deferred_arm() {
    let mut c = Controller::new(Some(calibration()), 1150);
    for i in 0..STATIONARY_SAMPLES + 100 {
        c.step(Some(sample()), Some(3300), false, i * 2000);
    }
    let now = (STATIONARY_SAMPLES + 100) * 2000;
    command(&mut c, 1, false, 0., false, now);
    command(&mut c, 2, true, 0., true, now);
    for i in 1..600 {
        command(&mut c, i + 2, true, 0., true, now + i as u32 * 2000);
        c.step(Some(sample()), Some(3300), true, now + i as u32 * 2000);
        assert!(!c.gate);
    }
    assert_eq!(c.core.state, FlightState::Disarmed);
}

#[test]
fn malformed_calibration_endpoints_are_rejected_without_integer_overflow() {
    assert!(
        !Calibration {
            raw_low: u16::MAX,
            raw_high: 2000,
            ..calibration()
        }
        .valid()
    );
}
