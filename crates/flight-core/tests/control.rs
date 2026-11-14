use flight_core::{
    CALIBRATION_SAMPLES, DT, FlightController,
    commands::Command,
    imu::{G, ImuSample, decode_mpu6050},
    math::{Quaternion, Vec3},
    motors::{MotorOutputs, mix},
    pid::{Gains, Pid},
    safety::{FlightState, StopReason},
};

fn stationary() -> ImuSample {
    ImuSample {
        accel: Vec3::new(0., 0., G),
        gyro: Vec3::new(0.01, -0.02, 0.005),
        temperature_c: 30.,
    }
}
fn ready() -> FlightController {
    let mut fc = FlightController::default();
    for n in 0..CALIBRATION_SAMPLES {
        fc.step(Some(stationary()), 12.6, n as u64 * 2);
    }
    fc
}
#[test]
fn decodes_signed_mpu6050_registers_in_si_units() {
    let mut b = [0; 14];
    for (i, v) in [(0, -16384i16), (4, 16384), (6, -340), (8, -131), (10, 262)] {
        b[i..i + 2].copy_from_slice(&v.to_be_bytes());
    }
    let s = decode_mpu6050(b);
    assert!((s.accel.x + G).abs() < 0.0001);
    assert!((s.accel.z - G).abs() < 0.0001);
    assert!((s.gyro.x + 1f32.to_radians()).abs() < 0.00001);
    assert!((s.gyro.y - 2f32.to_radians()).abs() < 0.00001);
    assert!((s.temperature_c - 35.53).abs() < 0.001);
}
#[test]
fn calibration_requires_consecutive_stationary_samples_and_blocks_early_arm() {
    let mut fc = FlightController::default();
    assert!(
        fc.accept_command(
            Command {
                arm: true,
                ..Default::default()
            },
            0
        )
        .is_err()
    );
    for n in 0..100 {
        fc.step(Some(stationary()), 12.6, n * 2);
    }
    fc.step(None, 12.6, 200);
    assert_eq!(fc.calibration_samples, 0);
    for n in 0..100 {
        fc.step(Some(stationary()), 12.6, 202 + n * 2);
    }
    let mut moving = stationary();
    moving.gyro.x = 1.;
    fc.step(Some(moving), 12.6, 200);
    assert_eq!(fc.calibration_samples, 0);
    assert_eq!(fc.motors.pwm_us, [1000; 4]);
    let fc = ready();
    assert_eq!(fc.state, FlightState::Disarmed);
    assert!((fc.gyro_bias.x - 0.01).abs() < 0.00001);
}
#[test]
fn high_throttle_arm_requires_release_before_retry() {
    let mut fc = ready();
    let mut c = Command {
        sequence: 1,
        arm: true,
        throttle: 0.5,
        ..Default::default()
    };
    fc.accept_command(c, 1000).unwrap();
    fc.step(Some(stationary()), 12.6, 1000);
    assert_eq!(fc.state, FlightState::Disarmed);
    c.sequence += 1;
    c.throttle = 0.;
    fc.accept_command(c, 1002).unwrap();
    fc.step(Some(stationary()), 12.6, 1002);
    assert_eq!(fc.state, FlightState::Disarmed);
    c.sequence += 1;
    c.arm = false;
    fc.accept_command(c, 1004).unwrap();
    c.sequence += 1;
    c.arm = true;
    fc.accept_command(c, 1006).unwrap();
    fc.step(Some(stationary()), 12.6, 1006);
    assert_eq!(fc.state, FlightState::Armed);
}
#[test]
fn replay_and_invalid_packets_do_not_feed_the_watchdog() {
    let mut fc = ready();
    let c = Command {
        sequence: 1,
        arm: true,
        ..Default::default()
    };
    fc.accept_command(c, 1000).unwrap();
    fc.step(Some(stationary()), 12.6, 1000);
    assert_eq!(fc.state, FlightState::Armed);
    assert!(fc.accept_command(c, 1499).is_err());
    assert!(
        fc.accept_command(
            Command {
                sequence: 2,
                throttle: f32::NAN,
                ..c
            },
            1499
        )
        .is_err()
    );
    fc.step(Some(stationary()), 12.6, 1499);
    assert_eq!(fc.state, FlightState::Armed);
    fc.step(Some(stationary()), 12.6, 1500);
    assert_eq!(fc.reason, Some(StopReason::LinkTimeout));
    assert_eq!(fc.motors.pwm_us, [1000; 4]);
    fc.accept_command(Command { sequence: 3, ..c }, 1501)
        .unwrap();
    fc.step(Some(stationary()), 12.6, 1501);
    assert_eq!(fc.state, FlightState::Failsafe);
    fc.accept_command(
        Command {
            sequence: 4,
            arm: false,
            ..c
        },
        1502,
    )
    .unwrap();
    assert_eq!(fc.state, FlightState::Disarmed);
}
#[test]
fn missing_or_nonfinite_sensor_samples_cut_armed_outputs() {
    for sample in [
        None,
        Some(ImuSample {
            gyro: Vec3::new(f32::NAN, 0., 0.),
            ..stationary()
        }),
    ] {
        let mut fc = ready();
        fc.accept_command(
            Command {
                sequence: 1,
                arm: true,
                ..Default::default()
            },
            1000,
        )
        .unwrap();
        fc.step(Some(stationary()), 12.6, 1000);
        fc.step(sample, 12.6, 1002);
        assert_eq!(fc.reason, Some(StopReason::ImuFailure));
        assert_eq!(fc.motors.pwm_us, [1000; 4]);
    }
}
#[test]
fn voltage_gates_arming_and_critical_voltage_latches_stop() {
    let mut fc = ready();
    let c = Command {
        sequence: 1,
        arm: true,
        ..Default::default()
    };
    fc.accept_command(c, 1000).unwrap();
    fc.step(Some(stationary()), 10.7, 1000);
    assert_eq!(fc.state, FlightState::Disarmed);
    fc.accept_command(
        Command {
            sequence: 2,
            arm: false,
            ..c
        },
        1002,
    )
    .unwrap();
    fc.accept_command(Command { sequence: 3, ..c }, 1004)
        .unwrap();
    fc.step(Some(stationary()), 12., 1004);
    fc.step(Some(stationary()), 10.2, 1006);
    assert_eq!(fc.reason, Some(StopReason::LowBattery));
}
#[test]
fn mixer_matches_physical_torque_signs_and_is_bounded() {
    let m = mix(0.5, 0.1, 0., 0.).normalized;
    assert!(m[0] > m[1] && m[3] > m[2]);
    let m = mix(0.5, 0., 0.1, 0.).normalized;
    assert!(m[2] > m[0] && m[3] > m[1]);
    let m = mix(0.5, 0., 0., 0.1).normalized;
    assert!(m[0] > m[1] && m[2] > m[3]);
    for throttle in [0., 0.5, 1.] {
        for r in [-0.25, 0.25] {
            for p in [-0.25, 0.25] {
                let m = mix(throttle, r, p, 0.25);
                assert!(m.normalized.iter().all(|v| (0.0..=1.).contains(v)));
                assert!(m.pwm_us.iter().all(|v| (1000..=2000).contains(v)));
            }
        }
    }
    assert_eq!(
        MotorOutputs::from_normalized([f32::NAN, -1., 2., 0.5]).pwm_us,
        [1000, 1000, 2000, 1500]
    );
}
#[test]
fn quaternion_rotation_and_integration_preserve_geometry() {
    let q = Quaternion::from_euler(0.2, -0.3, 0.7);
    let v = Vec3::new(1., 2., 3.);
    assert!((q.conjugate().rotate(q.rotate(v)) - v).norm() < 0.00001);
    let mut q = Quaternion::default();
    for _ in 0..500 {
        q.integrate(Vec3::new(0., 0., 1.), DT);
    }
    assert!((q.euler().z - 1.).abs() < 0.0001);
}
#[test]
fn pid_recovers_after_saturation_and_tuning_is_disarmed_only() {
    let mut pid = Pid::default();
    for _ in 0..5000 {
        assert!(pid.update(100., 0., Gains::default(), DT).abs() <= 0.25);
    }
    assert!(pid.update(0., 0., Gains::default(), DT).abs() < 0.001);
    let mut fc = ready();
    assert!(
        fc.tune(Gains {
            kp: f32::NAN,
            ..Default::default()
        })
        .is_err()
    );
    fc.accept_command(
        Command {
            sequence: 1,
            arm: true,
            ..Default::default()
        },
        1000,
    )
    .unwrap();
    fc.step(Some(stationary()), 12., 1000);
    assert!(fc.tune(Gains::default()).is_err());
}
