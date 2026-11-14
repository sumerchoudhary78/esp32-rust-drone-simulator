//! Connectivity model for the workbench. Nets affect the running devices;
//! this is a functional circuit model, not a SPICE or switching simulation.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Esp32,
    Mpu6050,
    Esc,
    Motor,
    Battery,
    Regulator,
    Divider,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    pub id: String,
    pub kind: Kind,
    pub x: f32,
    pub y: f32,
    pub slot: Option<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub device: String,
    pub pin: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wire {
    pub from: Pin,
    pub to: Pin,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Circuit {
    pub version: u32,
    pub devices: Vec<Device>,
    pub wires: Vec<Wire>,
}

pub const BOARD_PINS: &[&str] = &[
    "3V3", "EN", "VP", "VN", "GPIO34", "GPIO35", "GPIO32", "GPIO33", "GPIO25", "GPIO26", "GPIO27",
    "GPIO14", "GPIO12", "GND1", "GPIO13", "SD2", "SD3", "CMD", "5V", "GND2", "GPIO23", "GPIO22",
    "TX0", "RX0", "GPIO21", "GND3", "GPIO19", "GPIO18", "GPIO5", "GPIO17", "GPIO16", "GPIO4",
    "GPIO0", "GPIO2", "GPIO15", "SD1", "SD0", "CLK",
];
pub fn pins(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Esp32 => BOARD_PINS,
        Kind::Mpu6050 => &["VCC", "GND", "SDA", "SCL", "AD0", "INT"],
        Kind::Esc => &["BAT+", "GND", "SIGNAL", "A", "B", "C"],
        Kind::Motor => &["A", "B", "C"],
        Kind::Battery => &["+", "-"],
        Kind::Regulator => &["IN", "GND", "OUT5V"],
        Kind::Divider => &["IN", "GND", "OUT"],
    }
}
pub fn gpio(pin: &str) -> Option<u8> {
    match pin {
        "VP" => Some(36),
        "VN" => Some(39),
        "TX0" => Some(1),
        "RX0" => Some(3),
        "SD2" => Some(9),
        "SD3" => Some(10),
        "CMD" => Some(11),
        "SD1" => Some(8),
        "SD0" => Some(7),
        "CLK" => Some(6),
        _ => pin.strip_prefix("GPIO")?.parse().ok(),
    }
}
pub fn output_gpio(pin: &str) -> Option<u8> {
    gpio(pin).filter(|n| *n < 34 && !(6..=11).contains(n))
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct Wiring {
    pub esp_powered: bool,
    pub imu_powered: bool,
    pub sda_gpio: Option<u8>,
    pub scl_gpio: Option<u8>,
    pub imu_address: u8,
    pub motor_gpios: [Option<u8>; 4],
    pub motors_connected: [bool; 4],
    pub adc_gpio: Option<u8>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Validation {
    pub valid: bool,
    pub flight_connected: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub wiring: Wiring,
}
impl Circuit {
    pub fn empty() -> Self {
        Self {
            version: 1,
            devices: vec![],
            wires: vec![],
        }
    }
    pub fn preset() -> Self {
        let mut c = Self::empty();
        for (id, kind, x, y, slot) in [
            ("esp", Kind::Esp32, 380., 165., None),
            ("imu", Kind::Mpu6050, 125., 215., None),
            ("battery", Kind::Battery, 30., 30., None),
            ("regulator", Kind::Regulator, 310., 30., None),
            ("divider", Kind::Divider, 555., 35., None),
        ] {
            c.devices.push(Device {
                id: id.into(),
                kind,
                x,
                y,
                slot,
            });
        }
        for slot in 1..=4 {
            for (kind, prefix, x) in [(Kind::Esc, "esc", 760.), (Kind::Motor, "motor", 1005.)] {
                c.devices.push(Device {
                    id: format!("{prefix}{slot}"),
                    kind,
                    x,
                    y: 40. + (slot as f32 - 1.) * 155.,
                    slot: Some(slot),
                });
            }
        }
        for (a, b) in [
            ("battery.+", "regulator.IN"),
            ("battery.-", "regulator.GND"),
            ("regulator.OUT5V", "esp.5V"),
            ("battery.-", "esp.GND1"),
            ("esp.3V3", "imu.VCC"),
            ("battery.-", "imu.GND"),
            ("esp.GPIO21", "imu.SDA"),
            ("esp.GPIO22", "imu.SCL"),
            ("battery.-", "imu.AD0"),
            ("battery.+", "divider.IN"),
            ("battery.-", "divider.GND"),
            ("divider.OUT", "esp.GPIO34"),
        ] {
            c.link(a, b);
        }
        for (slot, pin) in [(1, 25), (2, 26), (3, 27), (4, 14)] {
            c.link("battery.+", &format!("esc{slot}.BAT+"));
            c.link("battery.-", &format!("esc{slot}.GND"));
            c.link(&format!("esp.GPIO{pin}"), &format!("esc{slot}.SIGNAL"));
            for phase in ["A", "B", "C"] {
                c.link(
                    &format!("esc{slot}.{phase}"),
                    &format!("motor{slot}.{phase}"),
                );
            }
        }
        c
    }
    pub fn link(&mut self, a: &str, b: &str) {
        let pin = |s: &str| {
            let (device, pin) = s.split_once('.').expect("internal pin name");
            Pin {
                device: device.into(),
                pin: pin.into(),
            }
        };
        self.wires.push(Wire {
            from: pin(a),
            to: pin(b),
        });
    }
    pub fn validate(&self) -> Validation {
        let mut v = Validation {
            valid: false,
            flight_connected: false,
            errors: vec![],
            warnings: vec![],
            wiring: Wiring {
                imu_address: 0x68,
                ..Default::default()
            },
        };
        if self.version != 1 || self.devices.len() > 24 || self.wires.len() > 128 {
            v.errors.push(
                "Unsupported circuit version or circuit exceeds 24 devices / 128 wires".into(),
            );
            return v;
        }
        let mut indices = BTreeMap::new();
        let mut ids = BTreeSet::new();
        let mut slots = BTreeSet::new();
        for d in &self.devices {
            if d.id.is_empty()
                || d.id.len() > 32
                || !d
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                || !ids.insert(&d.id)
                || !d.x.is_finite()
                || !d.y.is_finite()
                || !(0. ..=1400.).contains(&d.x)
                || !(0. ..=900.).contains(&d.y)
            {
                v.errors
                    .push("Invalid or duplicate device ID / position".into());
                return v;
            }
            let slot = if matches!(d.kind, Kind::Esc | Kind::Motor) {
                match d.slot {
                    Some(s) if (1..=4).contains(&s) => s,
                    _ => {
                        v.errors
                            .push("ESCs and motors need a unique position 1–4".into());
                        return v;
                    }
                }
            } else {
                if d.slot.is_some() {
                    v.errors.push("Only ESCs and motors have positions".into());
                    return v;
                }
                0
            };
            if !slots.insert((format!("{:?}", d.kind), slot)) {
                v.errors.push("Duplicate component type / position".into());
                return v;
            }
            for pin in pins(d.kind) {
                indices.insert(
                    Pin {
                        device: d.id.clone(),
                        pin: (*pin).into(),
                    },
                    indices.len(),
                );
            }
        }
        let mut parent: Vec<usize> = (0..indices.len()).collect();
        fn root(parent: &[usize], mut n: usize) -> usize {
            while parent[n] != n {
                n = parent[n];
            }
            n
        }
        let mut union = |a: &Pin, b: &Pin| -> bool {
            if let (Some(a), Some(b)) = (indices.get(a), indices.get(b)) {
                let a = root(&parent, *a);
                let b = root(&parent, *b);
                parent[a] = b;
                true
            } else {
                false
            }
        };
        for wire in &self.wires {
            if [&wire.from, &wire.to].iter().any(|p| {
                self.devices
                    .iter()
                    .any(|d| d.id == p.device && d.kind == Kind::Esp32)
                    && ["SD2", "SD3", "CMD", "SD1", "SD0", "CLK", "EN"].contains(&p.pin.as_str())
            }) {
                v.errors.push(
                    "Flash pins and EN are reserved in this board model; leave them unconnected"
                        .into(),
                );
                return v;
            }
            if wire.from == wire.to || !union(&wire.from, &wire.to) {
                v.errors
                    .push("Wire references a missing pin or itself".into());
                return v;
            }
        }
        for d in self.devices.iter().filter(|d| d.kind == Kind::Esp32) {
            for pin in ["GND2", "GND3"] {
                union(
                    &Pin {
                        device: d.id.clone(),
                        pin: "GND1".into(),
                    },
                    &Pin {
                        device: d.id.clone(),
                        pin: pin.into(),
                    },
                );
            }
        }
        let find = |kind: Kind, slot: Option<u8>| {
            self.devices
                .iter()
                .find(|d| d.kind == kind && d.slot == slot)
        };
        let net = |d: Option<&Device>, pin: &str| {
            d.and_then(|d| {
                indices.get(&Pin {
                    device: d.id.clone(),
                    pin: pin.into(),
                })
            })
            .map(|n| root(&parent, *n))
        };
        let same = |a: Option<usize>, b: Option<usize>| a.is_some() && a == b;
        let board = find(Kind::Esp32, None);
        let bat = find(Kind::Battery, None);
        let reg = find(Kind::Regulator, None);
        let imu = find(Kind::Mpu6050, None);
        let div = find(Kind::Divider, None);
        let ground = net(bat, "-");
        let battery = net(bat, "+");
        let five = net(reg, "OUT5V");
        let three = net(board, "3V3");
        let analog = net(div, "OUT");
        let rails = [
            ("battery +", battery),
            ("ground", ground),
            ("5 V", five),
            ("3.3 V", three),
            ("divider output", analog),
        ];
        for (i, (a, an)) in rails.iter().enumerate() {
            for (b, bn) in rails.iter().skip(i + 1) {
                if same(*an, *bn) {
                    v.errors
                        .push(format!("Short circuit: {a} connected to {b}"));
                }
            }
        }
        let board_gpio = |n: Option<usize>| -> Option<u8> {
            let b = board?;
            let matched: Vec<_> = pins(Kind::Esp32)
                .iter()
                .filter(|p| same(n, net(Some(b), p)))
                .filter_map(|p| output_gpio(p))
                .collect();
            if matched.len() == 1 {
                Some(matched[0])
            } else {
                None
            }
        };
        for d in &self.devices {
            let prohibited = match d.kind {
                Kind::Esc => vec!["SIGNAL", "A", "B", "C"],
                Kind::Motor => vec!["A", "B", "C"],
                Kind::Mpu6050 => vec!["SDA", "SCL", "INT"],
                _ => vec![],
            };
            for pin in prohibited {
                let n = net(Some(d), pin);
                if [battery, ground, five, three].iter().any(|r| same(n, *r)) {
                    v.errors
                        .push(format!("{}.{} connected to a power rail", d.id, pin));
                }
            }
        }
        if imu.is_some() && [battery, five].iter().any(|r| same(net(imu, "VCC"), *r)) {
            v.errors
                .push("MPU6050 VCC must use the modeled 3.3 V rail".into());
        }
        let mut phase_nets = BTreeSet::new();
        for d in self.devices.iter().filter(|d| d.kind == Kind::Esc) {
            for pin in ["A", "B", "C"] {
                if let Some(n) = net(Some(d), pin)
                    && !phase_nets.insert(n)
                {
                    v.errors
                        .push("ESC phase outputs are shorted together".into());
                }
            }
        }
        // No power rail may feed a GPIO, and output-capable GPIOs must not be tied together.
        if let Some(b) = board {
            let mut seen = BTreeSet::new();
            for p in pins(Kind::Esp32) {
                if gpio(p).is_some() {
                    let n = net(Some(b), p);
                    if [battery, ground, five, three].iter().any(|r| same(n, *r)) {
                        v.errors
                            .push(format!("GPIO pin {p} connected to a power rail"));
                    }
                    if let Some(n) = n
                        && !seen.insert(n)
                    {
                        v.errors.push("Multiple GPIO pins share a net".into());
                    }
                }
            }
        }
        v.wiring.esp_powered = board.is_some()
            && same(net(reg, "IN"), battery)
            && same(net(reg, "GND"), ground)
            && same(net(board, "5V"), five)
            && same(net(board, "GND1"), ground);
        v.wiring.imu_powered = v.wiring.esp_powered
            && imu.is_some()
            && same(net(imu, "VCC"), three)
            && same(net(imu, "GND"), ground);
        v.wiring.sda_gpio = board_gpio(net(imu, "SDA"));
        v.wiring.scl_gpio = board_gpio(net(imu, "SCL"));
        if same(net(imu, "SDA"), net(imu, "SCL")) {
            v.errors.push("MPU6050 SDA and SCL are shorted".into());
        }
        if same(net(imu, "AD0"), three) {
            v.wiring.imu_address = 0x69;
        } else if imu.is_some() && !same(net(imu, "AD0"), ground) {
            v.warnings
                .push("MPU6050 AD0 floats; this model defaults to address 0x68".into());
        }
        for i in 0..4 {
            let esc = find(Kind::Esc, Some(i as u8 + 1));
            let motor = find(Kind::Motor, Some(i as u8 + 1));
            v.wiring.motor_gpios[i] = board_gpio(net(esc, "SIGNAL"));
            let phases = ["A", "B", "C"]
                .iter()
                .all(|p| same(net(esc, p), net(motor, p)))
                && !same(net(esc, "A"), net(esc, "B"))
                && !same(net(esc, "B"), net(esc, "C"))
                && !same(net(esc, "A"), net(esc, "C"));
            v.wiring.motors_connected[i] = v.wiring.esp_powered
                && esc.is_some()
                && motor.is_some()
                && same(net(esc, "BAT+"), battery)
                && same(net(esc, "GND"), ground)
                && v.wiring.motor_gpios[i].is_some()
                && phases;
            if !v.wiring.motors_connected[i] {
                v.warnings.push(format!(
                    "M{} inactive: complete its ESC power, ground, signal and A/B/C phase wiring",
                    i + 1
                ));
            }
        }
        if div.is_some() && same(net(div, "IN"), battery) && same(net(div, "GND"), ground) {
            for p in ["VP", "VN", "GPIO32", "GPIO33", "GPIO34", "GPIO35"] {
                if same(net(board, p), analog) {
                    v.wiring.adc_gpio = gpio(p);
                }
            }
        }
        if !v.wiring.esp_powered {
            v.warnings
                .push("ESP32 needs battery → 5 V regulator → 5V and a common ground".into());
        }
        if !v.wiring.imu_powered || v.wiring.sda_gpio.is_none() || v.wiring.scl_gpio.is_none() {
            v.warnings
                .push("MPU6050 needs 3.3 V, ground, SDA and SCL".into());
        }
        if v.wiring.adc_gpio.is_none() {
            v.warnings
                .push("Battery ADC disconnected (100k / 27k divider → ADC1 pin)".into());
        }
        v.valid = v.errors.is_empty();
        v.flight_connected = v.valid
            && v.wiring.esp_powered
            && v.wiring.imu_powered
            && v.wiring.sda_gpio.is_some()
            && v.wiring.scl_gpio.is_some()
            && v.wiring.motors_connected.iter().all(|v| *v)
            && v.wiring.adc_gpio.is_some();
        v
    }
}
