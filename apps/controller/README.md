# Rust + Slint controller

The optional native UI controls **the local simulator**, through exactly the
same WebSocket contract as the browser. It has no serial, GPIO, Bluetooth or
physical-flight transport. The 3D view, virtual wiring, detailed plots, tuning,
fault injection and CSV capture are in the browser flight deck.

Desktop:

```sh
cargo run --manifest-path apps/controller/Cargo.toml
```

Start the main simulator separately (`cargo run --release` from the repository
root). Enter `ws://127.0.0.1:8787/ws` and Connect. Claiming control disarms the
simulation. Arm at zero throttle, then Take off. Roll/pitch/yaw sliders hold
their setpoints until **Center attitude controls**. The manual throttle slider
is enabled only when altitude assistance is off in the browser settings.

To run a dedicated virtual Android phone (requires the Android 11 ARM64 system
image; the script reuses the installed image but creates its own AVD):

```sh
bash scripts/build-android.sh
bash scripts/run-android-emulator.sh
```

This creates `Aerolab_Android_11` on emulator port 5580 and installs only this
development app. Pass `-no-window -gpu swiftshader_indirect` for headless checks.
The scripts default to the macOS Android Studio SDK/JDK paths; override
`ANDROID_HOME`, `ANDROID_NDK_ROOT`, `JAVA_HOME`, `ANDROID_BUILD_TOOLS`, or
`AEROLAB_ANDROID_IMAGE` when your installation differs.

Android supports the same library through `android_main`. The default endpoint
is `ws://10.0.2.2:8787/ws` for the official Android emulator. Use the computer's
LAN IP on a physical phone and bind the simulator to `0.0.0.0:8787`. INTERNET
permission and cleartext local transport are needed. Only use a trusted LAN.

Background/pause/focus loss suppresses heartbeats. Resume with an explicit
disarm/arm interaction; faults are never automatically rearmed. The network
worker is separate from the Slint event loop. Connection errors require an
explicit reconnect.

`python3 tests/android_smoke.py` exercises ownership, arming, takeoff, roll,
centering, Android background timeout, and explicit resume on the dedicated AVD.
It captures screenshots and a result record under `output/android/`.

The included `scripts/build-android.sh` packages a development APK using an
installed Android SDK/NDK, JDK and Rust Android target. It uses a local generated
debug keystore in ignored `output/android/`; it does not use any existing signing
identity. See the script output for the produced APK.

Alternatively the package metadata supports the official Slint documentation's
`cargo-apk` approach after installing that tool:

```sh
cd apps/controller
cargo apk build --target aarch64-linux-android --lib
```

Reference: [Slint's Rust Android backend and build requirements](https://docs.slint.dev/latest/docs/rust/slint/android/).
