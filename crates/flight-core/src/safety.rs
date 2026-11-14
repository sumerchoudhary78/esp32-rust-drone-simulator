use serde::Serialize;
pub const COMMAND_TIMEOUT_MS: u64 = 500;
pub const ARM_VOLTAGE: f32 = 10.8;
pub const CRITICAL_VOLTAGE: f32 = 10.2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlightState {
    Calibrating,
    Disarmed,
    Armed,
    Failsafe,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    LinkTimeout,
    ImuFailure,
    LowBattery,
    ExcessiveTilt,
    EmergencyStop,
    TimingOverrun,
}
