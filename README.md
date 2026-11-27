# Top Dungeon Farmer Defense

![Status: under development](https://img.shields.io/badge/status-under%20development-orange)

A cozy, colorful low-poly 3D tower defense with farming, built with [Bevy](https://bevy.org). Plant crops along the ants' paths, let your rabbit farmers harvest them, sell crops to the peddler, and place animal defenders to protect the warp crystal.

## Run

Linux needs ALSA, udev and Wayland headers (Debian/Ubuntu: `sudo apt install libasound2-dev libudev-dev libwayland-dev pkg-config`). See Bevy's [Linux dependencies](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md) for other distributions.

```sh
cargo run --features dev   # development: faster rebuilds, asset hot reloading
cargo run --release        # optimized build
```

## Checks

```sh
cargo +nightly fmt --all
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

## Documentation

- [`AGENTS.md`](AGENTS.md): how we work, the delivery workflow, and the Rust and Bevy conventions.
- [`REVIEW.md`](REVIEW.md): review rules and risk levels.
- `docs/designs/`: approved designs. `docs/adr/`: architecture decisions.
