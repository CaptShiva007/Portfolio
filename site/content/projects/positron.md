---
title: Positron
date: 2026-10-11
status: in-progress
tags: [rust, edr, endpoint-security, ebpf, etw, yara]
summary: A cross-platform, on-device EDR agent built in Rust.
---

# Positron

A cross-platform, on-device EDR (Endpoint Detection and Response) agent.
Detection runs locally on the endpoint — no dependency on cloud
connectivity for core detection and response — with results syncing to a
central management plane opportunistically.

## Status

Early scaffolding. Build order is deliberate: schema → one collector →
event bus → detection → response → sync. See `docs/architecture.md` for
the full rationale.

| Module         | Status         |
|-----------------|----------------|
| `schema`        | 🟢 done — `Event` type defined, tests passing |
| `collectors`     | 🟡 Windows/ETW process collector implemented; Linux/macOS stubbed |
| `bus`            | 🟢 done — bounded channel, tests passing |
| `detect`         | 🟡 Tier 1 (YARA) implemented; Tier 2/3 not started |
| `respond`        | 🟡 Windows kill/isolate implemented, not wired to detection; Linux/macOS stubbed |
| `store`          | ⚪ not started |
| `sync`           | ⚪ not started |
| `backend`        | ⚪ not started (much later) |

`main.rs` now runs the full pipe end to end: Windows collector → bus →
(Tier 1 YARA scan on process image, if resolvable) → stdout. No
storage/response yet — this proves the plumbing before building on top
of it.

## Why on-device detection

Positron targets offline resilience: the agent must be authoritative for
its own detect/respond decisions without waiting on a round trip to the
cloud. See `docs/architecture.md` for how this shapes the module
boundaries.

## Project layout

```
positron/
├── agent/              # The on-host agent (Rust workspace)
│   └── crates/
│       ├── schema/      # Normalized event types — the core contract
│       ├── collectors/  # OS-specific telemetry collection (eBPF/ETW/ESF)
│       ├── bus/         # In-process event distribution
│       ├── detect/      # Rule, behavioral, and ML detection tiers
│       ├── respond/     # Isolate/kill/rollback actions
│       ├── store/       # Local durable event log
│       └── sync/        # mTLS uplink to the management plane
├── backend/             # Management plane (built later)
├── proto/               # Shared schema definitions (source of truth)
├── rules/               # YARA rule files for Tier 1 detection
│   └── examples/         # Synthetic test rules (safe to run, no real signatures)
├── deploy/               # Packaging (docker, systemd, etc.)
└── docs/                 # Architecture and design docs
```

## Building

```bash
cd agent
cargo build
cargo test
```

# Positron Architecture

## Goals

- **Cross-platform**: Windows, Linux, macOS from day one.
- **On-device detection**: the agent makes detect/respond decisions
  locally, without waiting on cloud connectivity. The backend is for
  fleet visibility, policy distribution, and threat intel sync — not a
  dependency for core protection.

## System overview

```
┌─────────────────────────────────────────────────┐
│                  Management Plane                │
│  (Console, policy distribution, alert triage,     │
│   threat intel updates, fleet visibility)         │
└───────────────────┬───────────────────────────────┘
                     │ mTLS, async, batched
┌───────────────────▼───────────────────────────────┐
│              Positron Agent (per host)             │
│                                                     │
│  ┌──────────────┐   ┌──────────────┐               │
│  │ Collectors   │──▶│ Event Bus    │               │
│  │ (OS-specific)│   │ (local, ring │               │
│  └──────────────┘   │  buffer)     │               │
│                      └──────┬───────┘               │
│                             ▼                        │
│                  ┌────────────────────┐              │
│                  │ On-Device Detection │              │
│                  │ Engine              │              │
│                  │ - Rule/YARA match   │              │
│                  │ - Behavior graph    │              │
│                  │ - Local ML (light)  │              │
│                  └──────────┬──────────┘              │
│                             ▼                          │
│                  ┌────────────────────┐                │
│                  │ Response Actuator   │                │
│                  │ (kill/isolate/roll- │                │
│                  │  back — local exec) │                │
│                  └──────────┬──────────┘                │
│                             ▼                            │
│                  ┌────────────────────┐                  │
│                  │ Local Store + Sync  │                  │
│                  │ Queue (survives     │                  │
│                  │ offline periods)    │                  │
│                  └────────────────────┘                  │
└─────────────────────────────────────────────────────────┘
```

## Module build order (and why)

1. **`schema`** — the normalized `Event` type. Every other module depends
   on this contract. Get it wrong and every downstream module needs
   rework. Built first, deliberately, before any collector logic.
2. **`collectors`** — one platform fully (Linux/eBPF via Aya), others
   stubbed. Proves the schema survives contact with real event volume
   before we port the pattern to Windows/macOS.
3. **`bus`** — in-process pub/sub distributing normalized events from
   collectors to detection. Kept dumb initially (log to stdout/file).
4. **`detect`** — layered by cost: Tier 1 signature/IOC match (cheap,
   always-on) → Tier 2 behavioral graph over process lineage (catches
   multi-step chains) → Tier 3 local ML anomaly scoring (only on events
   that survive Tiers 1–2).
5. **`respond`** — isolate/kill/rollback, executed locally without
   waiting for a cloud ack.
6. **`store`** / **`sync`** — durable local queue (offline-first),
   opportunistic sync to the backend when connectivity returns.

## Key decisions

| Decision | Choice | Rationale |
|---|---|---|
| Language | Rust | Memory safety for a privileged agent, cross-compiles cleanly, no GC pauses in the hot path |
| eBPF loader | [Aya](https://aya-rs.dev/) | Pure Rust, no libbpf C dependency — keeps the stack Rust end-to-end |
| Async runtime | `tokio` | Needed for eBPF ring buffer polling + mTLS uplink without blocking detection |
| Schema format | `serde` + protobuf (`prost`) as wire format | Rust-native for the in-process path; protobuf as the versioned contract with the backend |
| Embedded store | `redb` or `sled` (TBD) | Pure Rust, avoids C bindings, consistent with the Aya choice |
| Workspace structure | Cargo workspace, one crate per module | Enforces dependency boundaries at compile time (e.g. `detect` cannot reach into `collectors` internals) |

## Platform-specific collection mechanisms

| Platform | Mechanism | Notes |
|---|---|---|
| Linux | eBPF (via Aya, CO-RE) | Built first — best tooling, fastest iteration, no driver signing |
| Windows | ETW (via `windows-rs`) | Second — matches native dev environment on Windows |
| macOS | Endpoint Security Framework (ESF) | Built last |

## Open questions / not yet decided

- `redb` vs `sled` for local store — needs a benchmark once `store` is
  actually built.
- Exact wire protocol for `sync` (gRPC via `tonic` vs plain HTTP/mTLS) —
  deferred until the backend exists.
- Self-protection / tamper resistance approach (kernel driver on
  Windows, ESF client protection on macOS) — needs research before
  `respond` ships.

## Changelog

- **Respond v0.1 (Windows only)**: `Responder` trait
  (`agent/crates/respond/src/responder.rs`) with a shared
  `is_pid_protected` safety net (refuses to act on PID 0/1/4 or the
  agent's own PID, regardless of platform). Windows implementation
  (`windows/mod.rs`): `kill_process` via `OpenProcess(PROCESS_TERMINATE)`
  + `TerminateProcess`; `isolate_host`/`deisolate_host` via `netsh
  advfirewall` block-all rules (blunt but simple — revisit with WFP for
  precision later). **Deliberately not wired into the detection path**
  — connecting a kill action straight to a Tier 1 YARA match (currently
  a synthetic test rule) is how you kill the wrong thing. Test manually
  via `cargo run --example manual_test -p positron-respond -- kill <pid>`
  (see `examples/manual_test.rs`) before any automated wiring happens.
  Linux/macOS remain stubs; rollback (undoing file/registry changes) is
  undesigned — needs change-tracking infrastructure this project
  doesn't have yet.
- **Detect v0.1 (Tier 1 only)**: `RuleEngine` (`agent/crates/detect/src/rules/mod.rs`)
  compiles `.yar`/`.yara` files from a directory using `yara-x` (pure-Rust
  YARA reimplementation, no `libyara` C dependency) and scans files
  against them. `main.rs` now loads rules from `rules/examples/` at
  startup and scans a process's image file whenever a `ProcessStart`
  event carries a resolvable path, printing any matches as
  `[DETECTION] pid=... image=... rule=...`. Tier 2 (behavioral graph)
  and Tier 3 (local ML) remain unimplemented — Tier 1 needs to prove
  itself against real event volume first.
- **Bus v0.1**: `EventBus` (`agent/crates/bus/src/lib.rs`) — bounded
  `tokio::mpsc` channel, `publish()` is the blocking-send side used by
  collector threads. Single consumer for now (plain `mpsc`, not
  `broadcast`) — revisit once `detect` and `store` both need the same
  stream. `main.rs` now wires the Windows collector's output through
  this bus and prints every event to stdout as a placeholder consumer.
  **Note:** this required changing `Collector::run`'s signature from a
  raw `tokio::sync::mpsc::UnboundedSender<Event>` to `positron_bus::EventBus`
  — the original signature predated the bus crate's design and didn't
  match once `bus` introduced backpressure via a bounded channel.
- **Collectors v0.1**: `Collector` trait defined (`agent/crates/collectors/src/collector.rs`).
  Windows implementation (`windows/mod.rs`) consumes the built-in
  `Microsoft-Windows-Kernel-Process` ETW provider via the `ferrisetw`
  crate, mapping process start/exit events into the schema. Requires
  running elevated. Self-heals from orphaned ETW sessions left by an
  ungraceful previous shutdown (`stop_trace_by_name` called before
  starting). Linux/eBPF and macOS/ESF remain stubs.
  ⚠️ ETW field names (`ImageName`, `CommandLine`, etc.) are unverified
  against a live trace — check with `wevtutil gp Microsoft-Windows-Kernel-Process /ge /gm`
  if fields come back empty at runtime.
- **Schema v0.1**: Initial `Event` type with `ProcessStart`,
  `ProcessExit`, `FileWrite`, `FileDelete`, `NetworkConnect`,
  `DnsQuery` variants. See `agent/crates/schema/src/event.rs`.