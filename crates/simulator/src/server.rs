use crate::{ClientMessage, World};
use axum::{
    Json, Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant},
};
use tokio::sync::{oneshot, watch};

enum Input {
    Connect(u64),
    Disconnect(u64),
    Message(
        u64,
        ClientMessage,
        oneshot::Sender<Result<(), &'static str>>,
    ),
}
#[derive(Clone)]
struct AppState {
    sender: SyncSender<Input>,
    latest: watch::Receiver<Published>,
    next_id: Arc<AtomicU64>,
}
#[derive(Clone)]
struct Published {
    json: Arc<String>,
    at: Instant,
}

pub async fn run(address: &str) -> Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    let (sender, receiver) = mpsc::sync_channel(128);
    let initial = Published {
        json: Arc::new(serde_json::to_string(&World::default().snapshot())?),
        at: Instant::now(),
    };
    let (publish, latest) = watch::channel(initial);
    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let thread_running = running.clone();
    let flight_thread = std::thread::Builder::new()
        .name("virtual-flight-500hz".into())
        .spawn(move || {
            let mut world = World::default();
            let period = Duration::from_micros(2000);
            let mut next = Instant::now();
            let mut measured_at = Instant::now();
            let mut measured_ticks = 0;
            while thread_running.load(Ordering::Relaxed) {
                // Bounded work; no socket reads, blocking sends or shared locks here.
                for _ in 0..64 {
                    let Ok(input) = receiver.try_recv() else {
                        break;
                    };
                    match input {
                        Input::Connect(id) => world.connect(id),
                        Input::Disconnect(id) => world.disconnect(id),
                        Input::Message(id, message, reply) => {
                            let result = world.handle(id, message);
                            let _ = reply.send(result);
                        }
                    }
                }
                let now = Instant::now();
                if now.saturating_duration_since(next) > Duration::from_millis(100) {
                    world.timing_overrun();
                    next = now;
                }
                world.step();
                measured_ticks += 1;
                if measured_at.elapsed() >= Duration::from_secs(1) {
                    world.measured_loop_hz =
                        measured_ticks as f32 / measured_at.elapsed().as_secs_f32();
                    measured_ticks = 0;
                    measured_at = Instant::now();
                }
                if world.tick % 20 == 0 {
                    let published = Published {
                        json: Arc::new(
                            serde_json::to_string(&world.snapshot()).expect("finite telemetry"),
                        ),
                        at: Instant::now(),
                    };
                    publish.send_replace(published);
                }
                next += period;
                if let Some(wait) = next.checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait);
                }
            }
        })?;
    let state = AppState {
        sender,
        latest,
        next_id: Arc::new(AtomicU64::new(1)),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/controller", get(index))
        .route("/app.js", get(app_js))
        .route("/scene.js", get(scene_js))
        .route("/styles.css", get(css))
        .route("/vendor/three.module.js", get(three))
        .route("/vendor/three.core.js", get(three_core))
        .route("/favicon.svg", get(icon))
        .route("/api/health", get(health))
        .route("/api/config", get(config))
        .route("/api/telemetry", get(telemetry))
        .route("/api/openapi.json", get(openapi))
        .route("/ws", get(ws))
        .with_state(state);
    println!(
        "\n  AEROLAB · ESP32 virtual flight lab\n  Open http://{}\n  Mode: SIMULATION ONLY · no hardware output\n  Ctrl+C to stop\n",
        listener.local_addr()?
    );
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    running.store(false, Ordering::Relaxed);
    let _ = flight_thread.join();
    result?;
    Ok(())
}
fn asset(content: &'static str, mime: &'static str) -> Response {
    ([(header::CONTENT_TYPE,mime),(header::CACHE_CONTROL,"no-cache"),(header::X_CONTENT_TYPE_OPTIONS,"nosniff"),(header::CONTENT_SECURITY_POLICY,"default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self' ws: wss:; img-src 'self' data:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'")],content).into_response()
}
async fn index() -> Response {
    asset(
        include_str!("../../../web/index.html"),
        "text/html; charset=utf-8",
    )
}
async fn app_js() -> Response {
    asset(
        include_str!("../../../web/app.js"),
        "text/javascript; charset=utf-8",
    )
}
async fn scene_js() -> Response {
    asset(
        include_str!("../../../web/scene.js"),
        "text/javascript; charset=utf-8",
    )
}
async fn css() -> Response {
    asset(
        include_str!("../../../web/styles.css"),
        "text/css; charset=utf-8",
    )
}
async fn three() -> Response {
    asset(
        include_str!("../../../web/vendor/three.module.js"),
        "text/javascript; charset=utf-8",
    )
}
async fn three_core() -> Response {
    asset(
        include_str!("../../../web/vendor/three.core.js"),
        "text/javascript; charset=utf-8",
    )
}
async fn icon() -> Response {
    asset(include_str!("../../../web/favicon.svg"), "image/svg+xml")
}
async fn openapi() -> Response {
    asset(
        include_str!("../../../docs/openapi.json"),
        "application/json",
    )
}
async fn telemetry(State(state): State<AppState>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        state.latest.borrow().json.as_ref().clone(),
    )
        .into_response()
}
async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let age = state.latest.borrow().at.elapsed().as_millis() as u64;
    (
        if age < 1000 {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(
            json!({"service":"aerolab","version":env!("CARGO_PKG_VERSION"),"mode":"simulation","nominal_loop_hz":500,"snapshot_age_ms":age,"healthy":age<1000}),
        ),
    )
}
async fn config() -> Json<serde_json::Value> {
    Json(json!({
        "protocol_version":1,"mode":"simulation","mass_kg":crate::physics::MASS_KG,"frame_diagonal_m":0.45,
        "max_thrust_per_motor_n":crate::physics::MAX_THRUST_N,"motor_time_constant_ms":45,"command_timeout_ms":500,
        "imu":{"model":"MPU6050","address":"0x68","accel_range_g":2,"gyro_range_deg_s":250,"sample_hz":500},
        "motors":[{"id":1,"position":"front_left","spin":"CW","suggested_gpio":25},{"id":2,"position":"front_right","spin":"CCW","suggested_gpio":26},{"id":3,"position":"rear_right","spin":"CW","suggested_gpio":27},{"id":4,"position":"rear_left","spin":"CCW","suggested_gpio":14}],
        "limitations":["Software-in-the-loop; no ESP32 CPU, electrical or RF emulation","Height assistance uses simulated ground truth","No GPS, absolute yaw or position hold","Model parameters and gains are not hardware-qualified","Motor-cut failsafe does not guarantee a safe landing"]
    }))
}
async fn ws(ws: WebSocketUpgrade, headers: HeaderMap, State(state): State<AppState>) -> Response {
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = origin.to_str().unwrap_or("");
        let host = headers
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        if origin
            .strip_prefix("http://")
            .or_else(|| origin.strip_prefix("https://"))
            != Some(host)
        {
            return (StatusCode::FORBIDDEN, "Cross-origin control is disabled").into_response();
        }
    }
    ws.max_message_size(16384)
        .max_frame_size(16384)
        .on_upgrade(move |socket| handle_socket(socket, state))
}
async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    if state.sender.try_send(Input::Connect(id)).is_err() {
        return;
    }
    if socket
        .send(Message::Text(
            json!({"type":"welcome","client_id":id}).to_string().into(),
        ))
        .await
        .is_err()
    {
        let _ = state.sender.send(Input::Disconnect(id));
        return;
    }
    let mut latest = state.latest.clone();
    let mut window = Instant::now();
    let mut messages = 0;
    loop {
        tokio::select! {
            changed=latest.changed()=>{
                if changed.is_err() {break;}
                let snapshot=latest.borrow_and_update().json.as_ref().clone();
                if !matches!(tokio::time::timeout(Duration::from_millis(250),socket.send(Message::Text(snapshot.into()))).await,Ok(Ok(()))) {break;}
            },
            incoming=socket.recv()=>{
                let Some(Ok(incoming))=incoming else {break};
                if window.elapsed()>=Duration::from_secs(1) {window=Instant::now();messages=0;}
                messages+=1;if messages>200 {break;}
                match incoming {
                    Message::Text(raw)=>{
                        let result=match serde_json::from_str::<ClientMessage>(&raw) {
                            Ok(message)=>{
                                let (reply,answer)=oneshot::channel();
                                if state.sender.try_send(Input::Message(id,message,reply)).is_err() {Err("Command queue full")}
                                else {match tokio::time::timeout(Duration::from_millis(250),answer).await {Ok(Ok(result))=>result,_=>Err("Flight task unavailable")}}
                            },
                            Err(_)=>Err("Invalid message · check protocol types, fields and finite values"),
                        };
                        if let Err(message)=result
                            && socket.send(Message::Text(json!({"type":"error","message":message}).to_string().into())).await.is_err() {break;}
                    },
                    Message::Close(_)=>break,
                    Message::Ping(data)=>{if socket.send(Message::Pong(data)).await.is_err(){break;}},
                    _=>{},
                }
            }
        }
    }
    // A full queue must not leave an abandoned controller lease behind. This
    // blocking channel operation is outside the flight thread and Tokio worker.
    let sender = state.sender.clone();
    let _ = tokio::task::spawn_blocking(move || sender.send(Input::Disconnect(id))).await;
}
