---
title: Rust on the ESP32-S3 with esp-hal
date: 2026-11-10
tags: [rust, esp32, esp-hal, embedded, cheatsheet]
summary: Toolchain install, project generation, flashing and the busy-wait patterns I use for bare-metal Rust on the ESP32-S3.
---

The setup I use for [IoT on the ShrikeFi](/projects/iot). The ESP32-S3 is an Xtensa chip, so stock `rustup` targets won't build for it. You need Espressif's Rust fork.

## Install once

```bash
cargo install espup espflash esp-generate
espup install
```

`espup` installs the `esp` toolchain channel. Each generated project pins it in `rust-toolchain.toml`:

```toml
[toolchain]
channel = "esp"
```

## New project

```bash
esp-generate --chip esp32s3 blinky
```

You get a `no_std` / `no_main` crate with:

- `.cargo/config.toml` that sets `target = "xtensa-esp32s3-none-elf"`, `build-std = ["core"]`, and `espflash flash --monitor --chip esp32s3` as the runner
- `.cargo/esp-config.toml` for runtime settings such as `ESP_LOG="info"`
- `opt-level = "s"` even in the dev profile, because unoptimized builds are too slow and too big for the chip

## Build, flash, watch

```bash
cargo run --release
```

That builds the image, flashes it over USB and opens the serial monitor. `log::info!` output shows up there through `esp-println`.

## Patterns

Minimal `main`:

```rust
esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    let mut led = Output::new(p.GPIO17, Level::Low, OutputConfig::default());
    loop { led.toggle(); /* wait */ }
}
```

Busy-wait without a timer peripheral:

```rust
let start = Instant::now();
while start.elapsed() < Duration::from_millis(1500) {}
```

Display drivers like `mipidsi` want an `embedded_hal::delay::DelayNs`. A spinning one is enough to get started:

```rust
struct BusyDelay;
impl DelayNs for BusyDelay {
    fn delay_ns(&mut self, ns: u32) {
        let wait = Duration::from_micros((ns as u64).div_ceil(1000));
        let start = Instant::now();
        while start.elapsed() < wait {}
    }
}
```

## Gotchas

- **The `target/` folder gets big.** `build-std` compiles `core` per project, and each of my three small projects reached about 1.5 GB. Keep `target/` in `.gitignore` and run `cargo clean` on projects you're done with.
- **Peripherals are moved, not borrowed.** `esp_hal::init` hands back each pin and bus once. Pass `peripherals.GPIO17` into `Output::new` and it's gone, so the compiler stops two drivers from grabbing the same pin.
