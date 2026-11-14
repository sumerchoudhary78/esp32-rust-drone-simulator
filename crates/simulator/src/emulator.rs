//! A separate process executes guest instructions. Its blocking pipe I/O never
//! runs on the simulation thread; stale or missing results inhibit propulsion.
use crate::{circuit::Wiring, physics::Plant};
use flight_core::{imu::ImuSample, motors::MotorOutputs};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

pub fn backend_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../output/emulator-build/aerolab-esp32")
}
pub fn flash_tool_path() -> PathBuf {
    let arch = std::env::consts::ARCH;
    let host = if cfg!(target_os = "macos") {
        format!("{arch}-apple-darwin")
    } else {
        format!("{arch}-unknown-linux-gnu")
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../output/esp32-tools")
        .join(host)
        .join("espflash")
}
#[derive(Clone, Debug)]
pub struct Image {
    pub id: String,
    pub name: String,
    pub bin: PathBuf,
    pub elf: Option<PathBuf>,
}
#[derive(Clone, Copy)]
struct SensorInput {
    imu: ImuSample,
    voltage: f32,
    powered: bool,
}
#[derive(Debug, Deserialize)]
pub struct Frame {
    pub cycles: u64,
    pub guest_us: u64,
    pub i2c_transactions: u64,
    pub imu_reads: u64,
    pub halted: bool,
    pub unmapped: u64,
    pub unhandled: u64,
    pub resets: u32,
    pub pwm_us: Vec<f32>,
    pub uart_hex: String,
}
#[derive(Clone, Debug, Serialize, Default)]
pub struct Status {
    pub running: bool,
    pub name: String,
    pub cycles: u64,
    pub guest_time_s: f64,
    pub i2c_transactions: u64,
    pub imu_reads: u64,
    pub unmapped: u64,
    pub unhandled: u64,
    pub output_age_ms: Option<u64>,
    pub message: String,
    pub uart: Vec<String>,
}
pub struct Runtime {
    child: Child,
    input: SyncSender<SensorInput>,
    uart: SyncSender<Vec<u8>>,
    output: Receiver<Result<Frame, String>>,
    active: Arc<AtomicBool>,
    started: Instant,
    last: Option<Instant>,
    wiring: Wiring,
    pub status: Status,
    outputs: MotorOutputs,
    log: VecDeque<String>,
    partial: String,
    waiting: bool,
}
impl Runtime {
    pub fn needs_sample(&self) -> bool {
        !self.waiting
    }
    pub fn start(image: Image, wiring: Wiring) -> Result<Self, String> {
        if !backend_path().is_file() {
            return Err("Install the CPU backend: python3 scripts/setup_emulator.py".into());
        }
        let adc = match wiring.adc_gpio {
            Some(36) => 0,
            Some(39) => 3,
            Some(32) => 4,
            Some(33) => 5,
            Some(34) => 6,
            Some(35) => 7,
            _ => -1,
        };
        let mut cmd = Command::new(backend_path());
        cmd.arg(&image.bin)
            .arg(wiring.sda_gpio.map_or(-1, i32::from).to_string())
            .arg(wiring.scl_gpio.map_or(-1, i32::from).to_string())
            .arg(wiring.imu_address.to_string())
            .arg(adc.to_string())
            .arg(u8::from(wiring.imu_powered).to_string());
        // Guest images are local, user-selected programs. No shell, host
        // networking, serial devices or user-chosen executable path is used.
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut writer = child.stdin.take().ok_or("Missing emulator stdin")?;
        let mut reader = BufReader::new(child.stdout.take().ok_or("Missing emulator stdout")?);
        let (input, rx) = mpsc::sync_channel::<SensorInput>(1);
        let (uart, urx) = mpsc::sync_channel::<Vec<u8>>(8);
        let (tx, output) = mpsc::sync_channel(8);
        let active = Arc::new(AtomicBool::new(true));
        let running = active.clone();
        std::thread::Builder::new()
            .name("esp32-cpu-pipes".into())
            .spawn(move || {
                while running.load(Ordering::Relaxed) {
                    let Ok(s) = rx.recv_timeout(Duration::from_millis(100)) else {
                        continue;
                    };
                    let bytes = urx.try_recv().unwrap_or_default();
                    let hex = if bytes.is_empty() {
                        "-".into()
                    } else {
                        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
                    };
                    let a = s.imu.accel;
                    let g = s.imu.gyro;
                    let adc = (s.voltage * 27. / 127. / 3.3 * 4095.).clamp(0., 4095.) as u16;
                    let result = (|| -> Result<Frame, String> {
                        writeln!(
                            writer,
                            "2000 {} {} {} {} {} {} {} {} {}",
                            a.x,
                            a.y,
                            a.z,
                            g.x,
                            g.y,
                            g.z,
                            adc,
                            u8::from(s.powered),
                            hex
                        )
                        .map_err(|e| e.to_string())?;
                        writer.flush().map_err(|e| e.to_string())?;
                        let mut line = String::new();
                        // Bounded read: a broken/native backend cannot allocate an
                        // unbounded JSON frame on the simulator process.
                        let read = (&mut reader)
                            .take(32769)
                            .read_line(&mut line)
                            .map_err(|e| e.to_string())?;
                        if read == 0 || read > 32768 || !line.ends_with('\n') {
                            return Err("CPU backend closed or exceeded the frame limit".into());
                        }
                        let frame: Frame = serde_json::from_str(&line)
                            .map_err(|e| format!("CPU backend protocol: {e}"))?;
                        if frame.pwm_us.len() != 40
                            || frame.pwm_us.iter().any(|v| !v.is_finite())
                            || frame.uart_hex.len() > 4096
                        {
                            return Err("Invalid CPU output frame".into());
                        }
                        Ok(frame)
                    })();
                    let failed = result.is_err();
                    if tx.send(result).is_err() || failed {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            child,
            input,
            uart,
            output,
            active,
            started: Instant::now(),
            last: None,
            wiring,
            status: Status {
                running: true,
                name: image.name,
                message: "Starting ESP32 CPU · motor outputs inhibited until fresh data".into(),
                ..Default::default()
            },
            outputs: MotorOutputs::STOP,
            log: VecDeque::new(),
            partial: String::new(),
            waiting: false,
        })
    }
    pub fn send_uart(&self, text: &str) -> Result<(), &'static str> {
        if text.len() > 255 || text.contains('\0') {
            return Err("UART input exceeds 255 bytes or includes NUL");
        }
        self.uart
            .try_send(text.as_bytes().to_vec())
            .map_err(|_| "UART queue is full")
    }
    pub fn stop(&mut self, reason: &str) {
        self.status.running = false;
        self.status.message = reason.into();
        self.outputs = MotorOutputs::STOP;
        self.active.store(false, Ordering::Relaxed);
        let _ = self.child.kill();
    }
    pub fn step(&mut self, imu: ImuSample, voltage: f32, powered: bool) -> (MotorOutputs, bool) {
        if !self.status.running {
            return (MotorOutputs::STOP, true);
        }
        if !self.waiting {
            self.waiting = self
                .input
                .try_send(SensorInput {
                    imu,
                    voltage,
                    powered,
                })
                .is_ok();
        }
        let mut advanced = false;
        while let Ok(result) = self.output.try_recv() {
            self.waiting = false;
            advanced = true;
            match result {
                Err(e) => {
                    self.stop(&e);
                    break;
                }
                Ok(f) => {
                    self.last = Some(Instant::now());
                    self.status.cycles = f.cycles;
                    self.status.guest_time_s = f.guest_us as f64 / 1_000_000.;
                    self.status.i2c_transactions = f.i2c_transactions;
                    self.status.imu_reads = f.imu_reads;
                    self.status.unmapped = f.unmapped;
                    self.status.unhandled = f.unhandled;
                    for chunk in f.uart_hex.as_bytes().as_chunks::<2>().0 {
                        if let Ok(pair) = std::str::from_utf8(chunk)
                            && let Ok(byte) = u8::from_str_radix(pair, 16)
                        {
                            if byte == b'\n' || self.partial.len() >= 240 {
                                self.log.push_back(std::mem::take(&mut self.partial));
                                while self.log.len() > 100 {
                                    self.log.pop_front();
                                }
                            } else if byte == b'\r' {
                            } else if byte.is_ascii() && !byte.is_ascii_control() {
                                self.partial.push(byte as char);
                            }
                        }
                    }
                    self.status.uart = self
                        .log
                        .iter()
                        .cloned()
                        .chain((!self.partial.is_empty()).then_some(self.partial.clone()))
                        .collect();
                    self.outputs = decode_outputs(&self.wiring, &f.pwm_us);
                    self.status.message =
                        "Executing guest firmware · UART / I²C / PWM / ADC functional models"
                            .into();
                    if f.halted || f.resets > 0 || f.unmapped > 0 || f.unhandled > 0 {
                        self.stop("Guest halted, reset, or accessed unmapped memory · outputs inhibited; inspect firmware compatibility");
                    }
                }
            }
        }
        self.status.output_age_ms = self.last.map(|t| t.elapsed().as_millis() as u64);
        if self
            .last
            .is_some_and(|t| t.elapsed() > Duration::from_millis(250))
            || self.started.elapsed() > Duration::from_secs(15) && self.last.is_none()
        {
            self.stop("CPU backend response timeout · outputs inhibited");
        }
        if !self.status.running
            || self.last.is_none()
            || self
                .last
                .is_some_and(|t| t.elapsed() > Duration::from_millis(100))
        {
            (MotorOutputs::STOP, advanced)
        } else {
            (self.outputs, advanced)
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Relaxed);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn decode_outputs(w: &Wiring, pwm: &[f32]) -> MotorOutputs {
    let mut out = MotorOutputs::STOP;
    for i in 0..4 {
        if w.motors_connected[i]
            && let Some(p) = w.motor_gpios[i].and_then(|p| pwm.get(p as usize))
            && p.is_finite()
            && (995. ..=2005.).contains(p)
        {
            let p = p.clamp(1000., 2000.);
            out.pwm_us[i] = p.round() as u16;
            out.normalized[i] = (p - 1000.) / 1000.;
        }
    }
    out
}

pub fn battery_voltage(plant: &Plant, low_battery: bool) -> f32 {
    if low_battery { 9.9 } else { plant.voltage() }
}
