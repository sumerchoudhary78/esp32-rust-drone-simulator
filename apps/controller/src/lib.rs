use serde_json::{Value, json};
use slint::ComponentHandle;
use std::{
    net::TcpStream,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};

slint::slint! {
    import { Button, Slider, LineEdit, VerticalBox, HorizontalBox, ScrollView } from "std-widgets.slint";
    export component PilotWindow inherits Window {
        title: "Aerolab · Virtual controller";
        width: 410px; height: 760px;
        background: #f6f7f3;
        in-out property <string> endpoint: "ws://127.0.0.1:8787/ws";
        in-out property <string> status: "Connect to your local simulator";
        in-out property <string> flight-state: "DISCONNECTED";
        in-out property <string> altitude: "0.00 m";
        in-out property <string> battery: "— V";
        in-out property <bool> connected: false;
        in-out property <bool> owns-control: false;
        in-out property <bool> ready: false;
        in-out property <bool> armed: false;
        in-out property <bool> failsafe: false;
        in-out property <bool> paused: false;
        in-out property <bool> assisted: true;
        in-out property <float> roll: 0;
        in-out property <float> pitch: 0;
        in-out property <float> yaw: 0;
        in-out property <float> throttle: 0;
        callback connect-to(string);
        callback set-arm(bool);
        callback action(string);
        callback pilot-changed(float,float,float,float);
        callback center-sticks();
        ScrollView {
            width: parent.width; height: parent.height;
            VerticalBox {
                padding: 23px; spacing: 14px;
                Text {text:"aerolab / pilot";font-size:28px;font-weight:700;color:#293b2b;}
                Text {text:"VIRTUAL ESP32 · NO PHYSICAL OUTPUT";font-size:10px;color:#9c7958;}
                LineEdit {text <=> root.endpoint;placeholder-text:"ws://computer-ip:8787/ws";}
                HorizontalBox {
                    Button {text:"Connect";clicked=>{root.connect-to(root.endpoint);}}
                    Button {text:"Take control";enabled:root.connected;clicked=>{root.action("claim_control");}}
                }
                Text {text:root.status;wrap:word-wrap;font-size:11px;color:#708064;}
                Rectangle {
                    height:85px;border-radius:10px;background:#e9efe1;
                    VerticalBox {padding:14px;spacing:6px;
                        Text {text:root.flight-state;color:#597146;font-size:13px;font-weight:600;}
                        HorizontalBox {Text {text:root.altitude;font-size:23px;color:#2c4229;}Text {text:root.battery;font-size:23px;color:#2c4229;horizontal-alignment:right;}}
                    }
                }
                Button {
                    text:root.paused ? "Resume controls / disarm" : root.failsafe ? "Acknowledge & disarm" : root.armed ? "Disarm motors" : "Arm motors";
                    enabled:root.owns-control && (root.ready || root.armed || root.failsafe || root.paused);
                    clicked=>{root.set-arm(!root.armed && !root.failsafe && !root.paused);}
                }
                HorizontalBox {
                    Button {text:"Take off";enabled:root.owns-control && root.armed && root.assisted;clicked=>{root.action("takeoff");}}
                    Button {text:"Land";enabled:root.owns-control && root.armed;clicked=>{root.action("land");}}
                }
                Text {text:"ROLL / PITCH / YAW";font-size:10px;color:#88977c;}
                HorizontalBox {Text {text:"Roll";width:50px;color:#637956;}Slider {minimum:-1;maximum:1;value <=> root.roll;enabled:root.owns-control;changed=>{root.pilot-changed(root.roll,root.pitch,root.yaw,root.throttle);}}}
                HorizontalBox {Text {text:"Pitch";width:50px;color:#637956;}Slider {minimum:-1;maximum:1;value <=> root.pitch;enabled:root.owns-control;changed=>{root.pilot-changed(root.roll,root.pitch,root.yaw,root.throttle);}}}
                HorizontalBox {Text {text:"Yaw";width:50px;color:#637956;}Slider {minimum:-1;maximum:1;value <=> root.yaw;enabled:root.owns-control;changed=>{root.pilot-changed(root.roll,root.pitch,root.yaw,root.throttle);}}}
                Button {text:"Center attitude controls";clicked=>{root.center-sticks();}}
                Text {text:root.assisted ? "Altitude assist active · virtual height sensor" : "Manual throttle · arm at zero";wrap:word-wrap;font-size:11px;color:#88977c;}
                Slider {minimum:0;maximum:1;value <=> root.throttle;enabled:root.owns-control && !root.assisted;changed=>{root.pilot-changed(root.roll,root.pitch,root.yaw,root.throttle);}}
                Button {text:"EMERGENCY STOP";enabled:root.connected && root.owns-control;clicked=>{root.action("emergency_stop");}}
                Text {text:"Closing or backgrounding this controller stops command transmission. The simulator watchdog cuts motors after 500 ms. This does not guarantee a safe landing.";wrap:word-wrap;font-size:10px;color:#98a08b;}
            }
        }
    }
}

enum Input {
    Connect(String),
    Arm(bool),
    Pilot(f32, f32, f32, f32),
    Action(String),
    #[cfg(target_os = "android")]
    Pause,
}
#[derive(Default)]
struct Pilot {
    roll: f32,
    pitch: f32,
    yaw: f32,
    throttle: f32,
    arm: bool,
}
type Socket = WebSocket<MaybeTlsStream<TcpStream>>;
fn publish_status(ui: &slint::Weak<PilotWindow>, text: impl Into<String>) {
    let text = text.into();
    let _ = ui.upgrade_in_event_loop(move |ui| ui.set_status(text.into()));
}
fn worker(receiver: Receiver<Input>, ui: slint::Weak<PilotWindow>) {
    let mut connection: Option<Socket> = None;
    let mut pilot = Pilot::default();
    let mut sequence = 0u64;
    let mut client_id = 0u64;
    let mut owner = false;
    let mut paused = false;
    let mut was_armed = false;
    let mut deadline = Instant::now();
    loop {
        let input = if connection.is_none() {
            match receiver.recv() {
                Ok(v) => Some(v),
                Err(_) => return,
            }
        } else {
            match receiver.try_recv() {
                Ok(v) => Some(v),
                Err(mpsc::TryRecvError::Disconnected) => return,
                Err(_) => None,
            }
        };
        if let Some(input) = input {
            match input {
                Input::Connect(endpoint) => {
                    connection = None;
                    pilot = Pilot::default();
                    sequence = 0;
                    owner = false;
                    paused = false;
                    if !endpoint.starts_with("ws://") {
                        publish_status(&ui, "Use a ws:// URL for the local simulator");
                        continue;
                    }
                    publish_status(&ui, "Connecting…");
                    match tungstenite::connect(endpoint.as_str()) {
                        Ok((socket, _)) => {
                            if let MaybeTlsStream::Plain(stream) = socket.get_ref() {
                                let _ = stream.set_nodelay(true);
                                let _ = stream.set_read_timeout(Some(Duration::from_millis(5)));
                                let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
                            }
                            connection = Some(socket);
                            publish_status(&ui, "Connected · waiting for telemetry");
                        }
                        Err(error) => publish_status(&ui, format!("Connection failed: {error}")),
                    }
                }
                Input::Arm(arm) => {
                    pilot.arm = arm;
                    paused = false;
                    if !arm {
                        pilot = Pilot::default();
                    }
                }
                Input::Pilot(r, p, y, t) => {
                    pilot.roll = r;
                    pilot.pitch = p;
                    pilot.yaw = y;
                    pilot.throttle = t;
                }
                #[cfg(target_os = "android")]
                Input::Pause => {
                    pilot = Pilot::default();
                    paused = true;
                }
                Input::Action(action) => {
                    if action == "emergency_stop" {
                        pilot = Pilot::default();
                    }
                    if let Some(socket) = connection.as_mut() {
                        let _ = socket.send(Message::Text(
                            json!({"type":"action","action":action}).to_string().into(),
                        ));
                    }
                }
            }
        }
        let mut disconnected = false;
        if let Some(socket) = connection.as_mut() {
            if owner && !paused && Instant::now() >= deadline {
                sequence += 1;
                deadline += Duration::from_millis(20);
                if deadline < Instant::now() {
                    deadline = Instant::now() + Duration::from_millis(20);
                }
                if socket.send(Message::Text(json!({"type":"command","sequence":sequence,"arm":pilot.arm,"roll":pilot.roll,"pitch":pilot.pitch,"yaw":pilot.yaw,"throttle":pilot.throttle}).to_string().into())).is_err() {disconnected=true;}
            }
            match socket.read() {
                Ok(Message::Text(raw)) => {
                    if let Ok(t) = serde_json::from_str::<Value>(&raw) {
                        match t["type"].as_str() {
                            Some("welcome") => client_id = t["client_id"].as_u64().unwrap_or(0),
                            Some("error") => publish_status(
                                &ui,
                                t["message"].as_str().unwrap_or("Protocol error"),
                            ),
                            Some("telemetry") => {
                                let new_owner = t["controller_id"].as_u64() == Some(client_id);
                                if owner != new_owner {
                                    pilot = Pilot::default();
                                }
                                owner = new_owner;
                                let armed = t["state"] == "armed";
                                let failsafe = t["state"] == "failsafe";
                                if was_armed
                                    && !armed
                                    && t["phase"] == "grounded"
                                    && t["state"] == "disarmed"
                                {
                                    pilot.arm = false;
                                }
                                was_armed = armed;
                                let ready = t["calibration_progress"].as_f64() == Some(1.)
                                    && t["battery_voltage"].as_f64().unwrap_or(0.) >= 10.8
                                    && t["command_age_ms"].as_u64().is_some_and(|age| age < 500)
                                    && t["faults"]["imu_failure"] != true;
                                let state = t["state"].as_str().unwrap_or("unknown").to_uppercase();
                                let altitude =
                                    format!("{:.2} m", t["position_m"]["z"].as_f64().unwrap_or(0.));
                                let battery =
                                    format!("{:.2} V", t["battery_voltage"].as_f64().unwrap_or(0.));
                                let assisted =
                                    t["settings"]["altitude_assist"].as_bool().unwrap_or(true);
                                let status = if failsafe {
                                    format!(
                                        "Failsafe: {} · clear fault and acknowledge",
                                        t["reason"].as_str().unwrap_or("unknown")
                                    )
                                } else if paused {
                                    "Paused · press Arm to resume".into()
                                } else if owner {
                                    "You have control · 50 Hz commands".into()
                                } else {
                                    "Observer · Take control disarms motors".into()
                                };
                                let _ = ui.upgrade_in_event_loop(move |ui| {
                                    ui.set_connected(true);
                                    ui.set_owns_control(owner);
                                    ui.set_ready(ready);
                                    ui.set_armed(armed);
                                    ui.set_failsafe(failsafe);
                                    ui.set_paused(paused);
                                    ui.set_flight_state(state.into());
                                    ui.set_altitude(altitude.into());
                                    ui.set_battery(battery.into());
                                    ui.set_assisted(assisted);
                                    ui.set_status(status.into());
                                });
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Message::Close(_)) => disconnected = true,
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => disconnected = true,
                _ => {}
            }
        }
        if disconnected {
            connection = None;
            owner = false;
            pilot = Pilot::default();
            let _ = ui.upgrade_in_event_loop(|ui| {
                ui.set_connected(false);
                ui.set_owns_control(false);
                ui.set_armed(false);
                ui.set_ready(false);
                ui.set_flight_state("DISCONNECTED".into());
                ui.set_status("Disconnected · connect again to resume".into());
            });
        }
    }
}

fn start(
    ui: PilotWindow,
    sender: Sender<Input>,
    receiver: Receiver<Input>,
) -> Result<(), Box<dyn std::error::Error>> {
    let weak = ui.as_weak();
    std::thread::Builder::new()
        .name("controller-network".into())
        .spawn(move || worker(receiver, weak))?;
    let tx = sender.clone();
    ui.on_connect_to(move |endpoint| {
        let _ = tx.send(Input::Connect(endpoint.to_string()));
    });
    let tx = sender.clone();
    let weak = ui.as_weak();
    ui.on_set_arm(move |arm| {
        if !arm && let Some(ui) = weak.upgrade() {
            ui.set_throttle(0.);
        }
        let _ = tx.send(Input::Arm(arm));
    });
    let tx = sender.clone();
    ui.on_pilot_changed(move |r, p, y, t| {
        let _ = tx.send(Input::Pilot(r, p, y, t));
    });
    let tx = sender.clone();
    let weak = ui.as_weak();
    ui.on_center_sticks(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_roll(0.);
            ui.set_pitch(0.);
            ui.set_yaw(0.);
            let _ = tx.send(Input::Pilot(0., 0., 0., ui.get_throttle()));
        }
    });
    ui.on_action(move |action| {
        let _ = sender.send(Input::Action(action.to_string()));
    });
    if let Ok(endpoint) = std::env::var("DRONE_WS") {
        ui.set_endpoint(endpoint.into());
    }
    ui.run()?;
    Ok(())
}
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::channel();
    start(PilotWindow::new()?, tx, rx)
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    use slint::android::android_activity::{MainEvent, PollEvent};
    let (tx, rx) = mpsc::channel();
    let lifecycle = tx.clone();
    slint::android::init_with_event_listener(app, move |event| {
        if matches!(
            event,
            PollEvent::Main(MainEvent::Pause | MainEvent::LostFocus)
        ) {
            let _ = lifecycle.send(Input::Pause);
        }
    })
    .expect("Android UI initialization");
    let ui = PilotWindow::new().expect("Controller window");
    ui.set_endpoint("ws://10.0.2.2:8787/ws".into());
    if let Err(error) = start(ui, tx, rx) {
        eprintln!("Aerolab controller: {error}");
    }
}
