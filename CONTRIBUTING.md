# Contributing

Start with the [README](README.md), [roadmap](ROADMAP.md) and
[protocol](docs/protocol.md). A focused bug report or reproducible experiment is
as useful as a new feature.

## Set up a local build

Use stable Rust with Clippy/rustfmt. The main simulator does not require Node,
Android tooling or an ESP32 board:

```sh
cargo run --release --locked
```

For integration checks, install Node.js 22+ and the development dependencies:

```sh
npm ci
npx playwright install --with-deps chrome
```

Before opening a pull request:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build -p drone-sim --locked
npm test
npm run test:browser
```

The protocol checks use port 8791; browser checks start their own server on 8790.
Keep those ports free. Your interactive simulator can stay on 8787.

The flight core also builds separately from the host server:

```sh
rustup target add wasm32-unknown-unknown
cargo check -p flight-core --target wasm32-unknown-unknown --locked
```

For changes to the optional native controller, run its checks separately and
describe the platforms actually exercised:

```sh
cargo fmt --manifest-path apps/controller/Cargo.toml -- --check
cargo clippy --manifest-path apps/controller/Cargo.toml --all-targets --locked -- -D warnings
```

See [native Android setup](apps/controller/README.md) for the dedicated emulator
check. Android SDK/NDK scripts have macOS defaults and support environment
overrides; do not assume those dependencies are installed on another machine.

## Keep changes reviewable

- Describe the concrete problem and resulting behavior. Include reproduction
  steps or the scenario/input that demonstrates it.
- Add a meaningful regression check when behavior changes. For plots or UI,
  include a screenshot and the viewport/device tested.
- Keep units, coordinate frames and motor order explicit. Preserve finite,
  bounded outputs and command expiry behavior.
- Keep virtual ground-truth sensors separate from the ordinary flight core.
  Do not describe simulation results as hardware qualification.
- Include generated build output, signing keys, local addresses, credentials and
  dependency caches only in your local ignored directories.
- Preserve third-party notices. Refresh vendored Three.js with `npm run vendor`
  only when intentionally updating its pinned dependency.

## Useful first contributions

Report a confusing setup step, document a reproducible control experiment, add a
missing boundary/fault scenario, or improve a keyboard/touch interaction with a
clear behavior check. Open an issue before starting a large architecture change.

Use [Issues](https://github.com/sumerchoudhary78/esp32-rust-drone-simulator/issues)
for focused bugs and proposals. Do not post credentials or private network data.
