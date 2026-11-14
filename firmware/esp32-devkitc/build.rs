fn main() {
    // Host policy tests do not link ESP32 hardware or its linker script.
    if std::env::var("TARGET").as_deref() == Ok("xtensa-esp32-none-elf") {
        println!("cargo:rustc-link-arg-bins=-Tlinkall.x");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
