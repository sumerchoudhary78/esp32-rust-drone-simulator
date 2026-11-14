#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut address = std::env::var("DRONE_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => address = args.next().ok_or("--bind requires host:port")?,
            "--help" | "-h" => {
                println!(
                    "AEROLAB · Rust virtual ESP32 drone\n\ncargo run --release -- [--bind 127.0.0.1:8787]\n\nUse --bind 0.0.0.0:8787 for Android browser access on a trusted LAN.\nRun deterministic scenarios with cargo test --workspace."
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    drone_sim::server::run(&address).await
}
