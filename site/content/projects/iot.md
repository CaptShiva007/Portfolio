---
title: IoT on the ShrikeFi
date: 2026-11-10
status: in-progress
tags: [iot, rust, esp32, esp-hal, embedded, fpga]
summary: Learning IoT from the bottom up with bare-metal Rust on an ESP32-S3 + FPGA board, heading toward LoRa networking.
---

This is where I'm learning IoT properly: not by gluing libraries together in Arduino, but by writing bare-metal Rust for the chip and working up from GPIO to buses, displays and radios. I'm most interested in the networking side, so LoRa is the first real goal.

Source: [github.com/CaptShiva007/IoT](https://github.com/CaptShiva007/IoT)

## The board

I'm using a [Vicharak ShrikeFi](https://store.vicharak.in/in/products/shrike-fi) (₹849). It puts an **ESP32-S3** (Wi-Fi + BLE) and a **Renesas ForgeFPGA** (1,120 LUTs) on one board, with 40 GPIO pins, a PMOD port and two USB-C ports. I wanted an ESP32 and an FPGA, and this board already has the two wired together for about the price of a plain ESP32 dev kit. The FPGA is small, but big enough for counters, PWM, UART and SPI peripherals.

## Where it stands

Three programs, all `no_std` Rust on [`esp-hal`](https://github.com/esp-rs/esp-hal) 1.2, built for `xtensa-esp32s3-none-elf`. There's no RTOS and no ESP-IDF underneath. All three build in release mode and flash with `espflash`.

| Project | What it does |
|---|---|
| `blinky` | Toggles the LED on GPIO17 every 1.5 s and logs `blink!` over USB serial |
| `breath` | Fades the same LED in and out with software PWM |
| `tft` | Drives a 240×320 ILI9341 SPI display: blue background, two lines of text, a red rectangle and a green circle |

## Design decisions

**Rust, no OS.** I already write Rust for [Positron](/projects/positron), and `esp-hal` makes me deal with the hardware directly: which peripheral, which pin, which clock. That's the part I want to understand. Every project starts from `esp-generate`, which sets up the Xtensa target, `build-std`, the bootloader app descriptor and an `espflash` runner, so `cargo run --release` builds, flashes and opens the serial monitor in one go.

**Busy-waiting for now, on purpose.** All the timing is `while start.elapsed() < d {}` against `esp_hal::time::Instant`. It burns the CPU, but it keeps the first programs to one idea each. Timers, interrupts and async come next.

**Breathing uses a squared duty curve.** Eyes don't see brightness linearly, so a linear ramp looks like it jumps to full and stays there. `breath` steps a level from 0 to 100 and back, and sets the duty to `level² / 100`. The PWM period is 1 ms (1 kHz, well above visible flicker), and each level is held for 10 periods. One full breath is 201 levels × 10 ms, about 2 seconds.

**The display goes through `mipidsi` + `embedded-graphics`.** SPI2 runs at 20 MHz with SCK on GPIO48, MOSI on GPIO47, CS on 18, DC on 16 and RESET on 15. `mipidsi` needs a `DelayNs` implementation, so I wrote a tiny `BusyDelay` that rounds nanoseconds up to microseconds and spins. This panel needs `ColorOrder::Bgr` and `flip_horizontal()` to get the colors and the text the right way round. `mipidsi` buffers pixel writes in a 518-byte buffer before each SPI transfer.

## Parts

Bought from robu.in for the next steps:

- 0.96" 128×64 OLED (SPI/I2C)
- RC522 13.56 MHz RFID reader (SPI)
- SX1278 Ra-02 433 MHz LoRa module
- 830-point breadboard, XD-42 5V/3.3V breadboard power supply, jumper wires
- 5mm LED kit with 220 Ω resistors, tactile buttons

A LoRa link needs a radio and an antenna at each end, so I need a second Ra-02, a second board to drive it, and two 433 MHz antennas. The Ra-02 also has edge pads that won't plug into a breadboard, so a soldering iron is on the list too.

## What's next

- Swap the busy-waits for the LEDC PWM peripheral and hardware timers.
- Read the RC522 over SPI, with a logic analyzer to watch the bus.
- Two Ra-02 radios talking point to point: first plain packets, then acknowledgements and retries.
- A kanji flashcard app on the TFT, with one large kanji and its reading per card, drawn from a bitmap font stored in flash.
- Get something running on the ForgeFPGA, then have the ESP32 talk to it over SPI.

Setup commands are in [Rust on the ESP32-S3 with esp-hal](/notes/esp32-s3-rust-esp-hal).
