use drone_sim::{Action, ClientMessage, Fault, World};
use flight_core::{
    commands::Command,
    math::Vec3,
    safety::{FlightState, StopReason},
};

fn run(w: &mut World, c: &mut Command, seconds: f32) {
    for _ in 0..(seconds * 500.) as usize {
        if w.tick.is_multiple_of(10) {
            c.sequence += 1;
            w.receive(*c).unwrap();
        }
        w.step();
    }
}
fn ready() -> (World, Command) {
    let mut w = World::default();
    w.connect(1);
    let mut c = Command::default();
    run(&mut w, &mut c, 1.2);
    assert_eq!(w.core.state, FlightState::Disarmed);
    (w, c)
}
fn flying() -> (World, Command) {
    let (mut w, mut c) = ready();
    c.arm = true;
    run(&mut w, &mut c, 0.1);
    assert_eq!(w.core.state, FlightState::Armed);
    w.action(Action::Takeoff).unwrap();
    run(&mut w, &mut c, 7.);
    (w, c)
}
#[test]
fn takeoff_holds_height_and_attitude_with_noisy_quantized_sensor() {
    let (w, _) = flying();
    assert_eq!(w.core.state, FlightState::Armed);
    assert!(
        (w.plant.position.z - 1.5).abs() < 0.08,
        "height {}",
        w.plant.position.z
    );
    assert!(
        w.core.attitude.x.abs() < 0.02 && w.core.attitude.y.abs() < 0.02,
        "attitude {:?}",
        w.core.attitude
    );
    assert!(
        w.plant
            .motor_speeds
            .iter()
            .all(|v| (*v - 0.673).abs() < 0.03)
    );
}

#[test]
fn natural_battery_depletion_eventually_latches_motor_cut() {
    let (mut w, mut c) = flying();
    run(&mut w, &mut c, 500.);
    assert!(!w.faults.low_battery);
    assert_eq!(w.core.reason, Some(StopReason::LowBattery));
    assert_eq!(w.core.motors.pwm_us, [1000; 4]);
    assert!(w.plant.position.finite() && w.plant.rates.finite());
}
#[test]
fn disturbance_is_rejected_and_roll_step_is_tracked() {
    let (mut w, mut c) = flying();
    w.action(Action::Disturb).unwrap();
    run(&mut w, &mut c, 3.);
    assert_eq!(w.core.state, FlightState::Armed);
    assert!(
        w.core.attitude.x.abs() < 0.04 && w.core.rates.norm() < 0.1,
        "{:?} {:?}",
        w.core.attitude,
        w.core.rates
    );
    c.roll = 0.5;
    run(&mut w, &mut c, 1.5);
    assert!(
        (w.core.attitude.x.to_degrees() - 10.).abs() < 2.,
        "roll {}",
        w.core.attitude.x.to_degrees()
    );
    assert!(w.plant.position.z > 1.2);
}
#[test]
fn assisted_landing_disarms_and_held_arm_cannot_relaunch() {
    let (mut w, mut c) = flying();
    w.action(Action::Land).unwrap();
    run(&mut w, &mut c, 8.);
    assert_eq!(w.core.state, FlightState::Disarmed);
    assert!(w.plant.position.z < 0.03);
    assert_eq!(w.core.motors.pwm_us, [1000; 4]);
    run(&mut w, &mut c, 1.);
    assert_eq!(w.core.state, FlightState::Disarmed);
}
#[test]
fn link_loss_cuts_motors_and_never_rearms_without_acknowledgment() {
    let (mut w, mut c) = flying();
    w.handle(
        1,
        ClientMessage::Fault {
            fault: Fault::LinkLoss,
            enabled: true,
        },
    )
    .unwrap();
    run(&mut w, &mut c, 0.55);
    assert_eq!(w.core.reason, Some(StopReason::LinkTimeout));
    assert_eq!(w.core.motors.pwm_us, [1000; 4]);
    w.handle(
        1,
        ClientMessage::Fault {
            fault: Fault::LinkLoss,
            enabled: false,
        },
    )
    .unwrap();
    run(&mut w, &mut c, 0.5);
    assert_eq!(w.core.state, FlightState::Failsafe);
    c.arm = false;
    run(&mut w, &mut c, 0.1);
    assert_eq!(w.core.state, FlightState::Disarmed);
}
#[test]
fn sensor_and_battery_faults_cut_within_one_control_step() {
    for fault in [Fault::ImuFailure, Fault::LowBattery] {
        let (mut w, _) = flying();
        w.handle(
            1,
            ClientMessage::Fault {
                fault,
                enabled: true,
            },
        )
        .unwrap();
        w.step();
        assert_eq!(w.core.state, FlightState::Failsafe);
        assert_eq!(w.core.motors.pwm_us, [1000; 4]);
    }
}
#[test]
fn packet_loss_and_expired_latency_do_not_feed_watchdog() {
    for (loss, latency) in [(100., 20), (0., 600)] {
        let (mut w, mut c) = flying();
        w.settings.packet_loss = loss;
        w.settings.latency_ms = latency;
        run(&mut w, &mut c, 1.);
        assert_eq!(w.core.reason, Some(StopReason::LinkTimeout));
    }
}
#[test]
fn emergency_stop_purges_delayed_arm_packets() {
    let (mut w, mut c) = flying();
    w.settings.latency_ms = 200;
    run(&mut w, &mut c, 0.1);
    w.action(Action::EmergencyStop).unwrap();
    run(&mut w, &mut c, 1.);
    assert_eq!(w.core.reason, Some(StopReason::EmergencyStop));
    assert_eq!(w.core.motors.pwm_us, [1000; 4]);
}
#[test]
fn ownership_transfer_disarms_and_observers_cannot_mutate() {
    let (mut w, _) = flying();
    w.connect(2);
    assert!(
        w.handle(
            2,
            ClientMessage::Action {
                action: Action::Reset
            }
        )
        .is_err()
    );
    assert_eq!(w.core.state, FlightState::Armed);
    w.handle(
        2,
        ClientMessage::Action {
            action: Action::ClaimControl,
        },
    )
    .unwrap();
    assert_eq!(w.core.state, FlightState::Disarmed);
    assert_eq!(w.controller_id, Some(2));
    assert!(
        w.handle(
            1,
            ClientMessage::Action {
                action: Action::Takeoff
            }
        )
        .is_err()
    );
}
#[test]
fn lost_connection_and_host_overrun_are_fail_closed() {
    let (mut w, _) = flying();
    w.disconnect(1);
    for _ in 0..260 {
        w.step();
    }
    assert_eq!(w.core.reason, Some(StopReason::LinkTimeout));
    let (mut w, _) = flying();
    w.timing_overrun();
    assert_eq!(w.core.reason, Some(StopReason::TimingOverrun));
    assert_eq!(w.core.motors.pwm_us, [1000; 4]);
}
#[test]
fn manual_throttle_and_wind_use_same_underlying_plant() {
    let (mut w, mut c) = ready();
    w.settings.altitude_assist = false;
    c.arm = true;
    run(&mut w, &mut c, 0.1);
    c.throttle = 0.72;
    run(&mut w, &mut c, 2.);
    assert!(w.plant.position.z > 1., "height {}", w.plant.position.z);
    w.settings.wind = 2.;
    run(&mut w, &mut c, 0.5);
    assert!(w.plant.velocity.x > 0.1);
}
#[test]
fn motor_failure_removes_actual_thrust_and_reset_clears_every_fault() {
    let (mut w, mut c) = flying();
    w.handle(
        1,
        ClientMessage::Fault {
            fault: Fault::MotorFailure,
            enabled: true,
        },
    )
    .unwrap();
    run(&mut w, &mut c, 0.3);
    assert!(w.plant.motor_speeds[0] < 0.002);
    assert!(w.plant.rates.norm() > 0.1);
    w.action(Action::Reset).unwrap();
    assert_eq!(w.plant.position, Vec3::ZERO);
    assert!(!w.faults.motor_failure);
    assert_eq!(w.core.state, FlightState::Calibrating);
    assert!(w.action(Action::Takeoff).is_err());
}
#[test]
fn malformed_wire_data_and_unsafe_tuning_are_rejected() {
    assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"command","sequence":1,"throttle":0,"roll":0,"pitch":0,"yaw":0,"arm":false,"extra":true}"#).is_err());
    let (mut w, _) = ready();
    assert!(
        w.handle(
            1,
            ClientMessage::Tune {
                kp: 99.,
                ki: 0.,
                kd: 0.
            }
        )
        .is_err()
    );
    assert!(
        w.handle(
            1,
            ClientMessage::Settings {
                altitude_assist: true,
                target_altitude: 999.,
                latency_ms: 0,
                packet_loss: 0.,
                wind: 0.
            }
        )
        .is_err()
    );
}
