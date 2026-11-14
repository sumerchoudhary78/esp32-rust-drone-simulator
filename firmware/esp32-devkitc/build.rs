fn main() {
    // Host policy tests do not link ESP32 hardware or its linker script.
    if std::env::var("TARGET").as_deref() == Ok("xtensa-esp32-none-elf") {
        println!("cargo:rustc-link-arg-bins=-Tlinkall.x");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=AEROLAB_BENCH_PROFILE");
    if std::env::var_os("CARGO_FEATURE_BENCH_FIRMWARE").is_some() {
        assert!(
            std::env::var_os("AEROLAB_BENCH_PROFILE").is_some(),
            "bench firmware requires a device profile; use scripts/esp32_firmware.py init-profile"
        );
    }
    let mut key = [0u8; 32];
    let mut calibration = "None".to_owned();
    let mut max_pwm = 1150;
    let mut esc_hz = 50;
    let mut simulated = false;
    let mut ssid = "Aerolab-ESP32".to_owned();
    let mut password = String::new();
    if let Ok(path) = std::env::var("AEROLAB_BENCH_PROFILE") {
        println!("cargo:rerun-if-changed={path}");
        let p: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("read bench profile"))
                .expect("valid profile JSON");
        assert_eq!(p["version"], 1, "profile version must be 1");
        let secret = p["psk_hex"].as_str().expect("profile psk_hex");
        assert_eq!(secret.len(), 64, "32-byte command key required");
        for (i, b) in key.iter_mut().enumerate() {
            *b = u8::from_str_radix(&secret[i * 2..i * 2 + 2], 16).expect("hex command key");
        }
        assert!(
            key.iter().any(|b| *b != 0),
            "zero command key is not allowed"
        );
        max_pwm = p["max_pwm_us"].as_u64().expect("max_pwm_us");
        assert!((1000..=2000).contains(&max_pwm));
        esc_hz = p["esc_hz"].as_u64().expect("esc_hz");
        assert!([50, 100, 200, 250, 400].contains(&esc_hz));
        simulated = p["mode"] == "emulated";
        assert!(simulated || p["mode"] == "bench" || p["mode"] == "experimental_flight");
        if p["mode"] == "bench" {
            assert!(max_pwm <= 1150, "bench profile must cap PWM at 1150 us");
        }
        if std::env::var_os("CARGO_FEATURE_BENCH_FAULT_INJECTION").is_some() {
            assert!(
                p["mode"] != "experimental_flight" && max_pwm <= 1150,
                "fault injection is only for capped bench images"
            );
        }
        if let Some(c) = p["battery_calibration"].as_object() {
            let low = c["raw_low"].as_u64().expect("raw_low");
            let high = c["raw_high"].as_u64().expect("raw_high");
            let lv = c["volts_low"].as_f64().expect("volts_low");
            let hv = c["volts_high"].as_f64().expect("volts_high");
            assert!(
                low > 32
                    && high < 4063
                    && high > low + 100
                    && (8.0..=11.5).contains(&lv)
                    && (11.5..=13.0).contains(&hv)
                    && hv - lv >= 1.0,
                "invalid ADC calibration points"
            );
            calibration = format!(
                "Some(crate::battery::Calibration {{raw_low:{low},raw_high:{high},volts_low:{lv:?}f32,volts_high:{hv:?}f32}})"
            );
        }
        ssid = p["wifi_ssid"]
            .as_str()
            .unwrap_or("Aerolab-ESP32")
            .to_owned();
        password = p["wifi_password"].as_str().unwrap_or("").to_owned();
        assert!(!ssid.is_empty() && ssid.len() <= 32);
    }
    if std::env::var_os("CARGO_FEATURE_BENCH_WIFI").is_some() {
        assert!(
            (12..=63).contains(&password.len()),
            "Wi-Fi profile needs a 12–63 character WPA2 password"
        );
        assert!(!simulated, "radio images require a real device profile");
    }
    let generated = format!(
        "pub const COMMAND_KEY:[u8;32]={key:?};pub const CALIBRATION:Option<crate::battery::Calibration>={calibration};pub const MAX_PWM_US:u16={max_pwm};pub const ESC_HZ:u32={esc_hz};pub const SIMULATED:bool={simulated};pub const WIFI_SSID:&str={ssid:?};pub const WIFI_PASSWORD:&str={password:?};"
    );
    std::fs::write(
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("bench_profile.rs"),
        generated,
    )
    .unwrap();
}
