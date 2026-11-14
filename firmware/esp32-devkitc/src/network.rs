//! Core 0 only. No network future, heap allocation or socket is used by Core 1.
use super::*;
use core::{
    fmt::Write,
    net::{Ipv4Addr, SocketAddrV4},
};
use embassy_net::{
    Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4,
    udp::{PacketMetadata, UdpSocket},
};
use embassy_time::{Duration as AsyncDuration, Timer, with_timeout};
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, ControllerConfig, Interface, WifiController,
    ap::AccessPointConfig,
};

pub async fn run(spawner: embassy_executor::Spawner, mut comms: Comms) -> ! {
    esp_alloc::heap_allocator!(size: 72 * 1024);
    let timers = TimerGroup::new(comms.timer);
    esp_rtos::start(timers.timer0, comms.interrupt);
    let device = Interface::access_point();
    let _controller = WifiController::new(
        comms.wifi,
        ControllerConfig::default().with_initial_config(Config::AccessPoint(
            AccessPointConfig::default()
                .with_ssid(profile::WIFI_SSID.try_into().unwrap())
                .with_authentication(AuthenticationMethodConfig::Wpa2Personal(
                    profile::WIFI_PASSWORD.try_into().unwrap(),
                ))
                .with_max_connections(1),
        )),
    )
    .unwrap();
    let config = embassy_net::Config::ipv4_static(StaticConfigV4 {
        address: Ipv4Cidr::new(Ipv4Addr::new(192, 168, 4, 1), 24),
        gateway: None,
        dns_servers: Default::default(),
    });
    static RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(
        device,
        config,
        RESOURCES.init(StackResources::new()),
        comms.session,
    );
    spawner.spawn(net_task(runner).unwrap());
    spawner.spawn(dhcp(stack).unwrap());
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 2];
    let mut rx = [0; 512];
    let mut tx = [0; 1024];
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx, &mut tx_meta, &mut tx);
    socket.bind(14550).unwrap();
    println!(
        "wifi ap={} address=192.168.4.1 udp=14550 dhcp=true command=AERO2",
        profile::WIFI_SSID
    );
    let mut frame = UartFrame::default();
    let mut next = now_us();
    let mut reply_at = now_us();
    let mut packet = [0; 128];
    loop {
        receive_uart(&mut comms.receiver, &mut frame);
        if let Ok(Ok((len, peer))) =
            with_timeout(AsyncDuration::from_millis(2), socket.recv_from(&mut packet)).await
        {
            if &packet[..len] == b"AERO2 HELLO" {
                // Status is public on this private AP. Rate-limit replies; it
                // carries a boot nonce and counters, never either secret.
                let now = now_us();
                if (now.wrapping_sub(reply_at) as i32) >= 0 {
                    reply_at = now.wrapping_add(100_000);
                    let mut status = heapless::String::<512>::new();
                    let _ = write!(
                        status,
                        "{{\"protocol\":2,\"session\":\"{:016x}\",\"next_sequence\":{},\"state\":{},\"calibration\":{},\"adc_raw\":{},\"voltage\":{:.3},\"gate\":{},\"watchdog\":{},\"roll\":{:.4},\"pitch\":{:.4},\"yaw\":{:.4},\"flight_qualified\":false}}",
                        comms.session,
                        comms.receiver.next_sequence(),
                        STATE.load(Ordering::Relaxed),
                        CALIBRATION.load(Ordering::Relaxed),
                        ADC_RAW.load(Ordering::Relaxed),
                        f32::from_bits(VOLTAGE.load(Ordering::Relaxed)),
                        GATE.load(Ordering::Relaxed),
                        WATCH_TRIPPED.load(Ordering::Relaxed),
                        f32::from_bits(MOTION[0].load(Ordering::Relaxed)),
                        f32::from_bits(MOTION[1].load(Ordering::Relaxed)),
                        f32::from_bits(MOTION[2].load(Ordering::Relaxed))
                    );
                    let _ = with_timeout(
                        AsyncDuration::from_millis(2),
                        socket.send_to(status.as_bytes(), peer),
                    )
                    .await;
                }
            } else {
                match comms.receiver.receive(&packet[..len], now_us()) {
                    Ok(command) => COMMANDS.publish(command),
                    Err(_) => {
                        REJECTED.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
        if (now_us().wrapping_sub(next) as i32) >= 0 {
            status(comms.session, comms.receiver.next_sequence());
            next = now_us().wrapping_add(100_000);
        }
        // Also yield under a continuous flood of immediately available packets.
        Timer::after(AsyncDuration::from_millis(1)).await;
    }
}
#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}
#[embassy_executor::task]
async fn dhcp(stack: Stack<'static>) {
    use edge_dhcp::{
        io::{self, DEFAULT_SERVER_PORT},
        server::{Server, ServerOptions},
    };
    use edge_nal::UdpBind;
    use edge_nal_embassy::{Udp, UdpBuffers};
    let ip = Ipv4Addr::new(192, 168, 4, 1);
    let mut buffer = [0; 1500];
    let mut gateway = [Ipv4Addr::UNSPECIFIED];
    let buffers = UdpBuffers::<1, 1024, 1024, 4>::new();
    let udp = Udp::new(stack, &buffers);
    let mut socket = udp
        .bind(core::net::SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            DEFAULT_SERVER_PORT,
        )))
        .await
        .unwrap();
    loop {
        let _ = io::server::run(
            &mut Server::<_, 4>::new_with_et(ip),
            &ServerOptions::new(ip, Some(&mut gateway)),
            &mut socket,
            &mut buffer,
        )
        .await;
        Timer::after(AsyncDuration::from_millis(500)).await;
    }
}
