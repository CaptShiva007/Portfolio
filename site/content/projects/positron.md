---
title: Positron
date: 2026-11-01
status: in-progress
tags: [rust, edr, endpoint-security, etw, yara, windows]
summary: A cross-platform EDR agent in Rust that detects and responds on the endpoint itself, no cloud round trip required.
---

Positron is an EDR (Endpoint Detection and Response) agent I'm building in Rust. The core idea: the agent on the host is authoritative for its own detect and respond decisions. A backend will exist eventually for fleet visibility and rule updates, but a host that loses its network connection should keep protecting itself.

Source: [github.com/CaptShiva007/positron](https://github.com/CaptShiva007/positron)

## Where it stands

The pipeline runs end to end on Windows: a process starts, ETW reports it, the image gets scanned against YARA rules, and a matching rule can kill the process.

| Module | Status |
|---|---|
| `schema` | Done. Normalized `Event` type: process start/exit, file write/delete, network connect, DNS query |
| `collectors` | Windows (ETW) process collector working. Linux (eBPF) and macOS (ESF) are empty stubs |
| `bus` | Done. Bounded `tokio::mpsc` channel with backpressure |
| `detect` | Tier 1 (YARA) working. Tier 2 (behavior graph) and Tier 3 (local ML) not started |
| `respond` | Windows kill and network isolation working, wired to detections. Linux/macOS stubbed |
| `store` / `sync` | Not started |
| `backend` | Not started, and deliberately last |

## How it fits together

```
ETW (Microsoft-Windows-Kernel-Process)
        │  ProcessStart / ProcessExit
        ▼
   collector thread ──publish()──▶ EventBus (bounded channel)
                                        │
                                        ▼
                               main loop: for each event
                                        │ ProcessStart with an image path
                                        ▼
                         spawn_blocking: YARA scan of the image
                                        │ match
                                        ▼
                     action = "kill"? ──▶ Responder::kill_process
                                          (only with --enforce)
```

Each module is its own crate in a Cargo workspace, so the compiler enforces the boundaries. `detect` can't reach into `collectors` internals even if I get lazy.

## Design decisions

**Rust, end to end.** An EDR agent runs with high privileges on every host, which makes memory-safety bugs in it a gift to attackers. I've also avoided C dependencies where a pure-Rust option exists: [`yara-x`](https://virustotal.github.io/yara-x/) instead of `libyara`, Aya for eBPF instead of `libbpf`, and `redb` or `sled` for the local store.

**Build order is the architecture.** Schema first, then one collector, then the bus, detection, response and sync. The `Event` type is the contract every other module depends on, so it had to exist and survive real event volume before anything was stacked on it.

**Detection is tiered by cost.** Tier 1 signature matching is cheap and always on. Tier 2 will build a graph over process lineage to catch multi-step attack chains. Tier 3 runs lightweight ML anomaly scoring, but only on events that get past the first two tiers.

**Destructive responses are opt-in, twice over.** A detection only kills a process if:

1. the rule itself asks for it with `action = "kill"` in its meta block (anything else, or nothing, means alert only), **and**
2. the agent was started with `--enforce`.

Without `--enforce`, the agent logs `would kill pid=...` instead. Hooking a kill action straight up to a pattern match is how you end up terminating the wrong thing on your own machine. Each process also gets killed at most once, even if several rules match it.

**Some PIDs are never touched.** PID 0 (System Idle), 4 (System), 1 (init/systemd), and the agent's own PID are refused regardless of what a rule says.

**Isolation keeps a way back in.** Cutting a compromised host off the network is easy. Cutting it off while still letting the management plane reach it, so you can undo the isolation later, is harder on Windows Firewall, where block rules always beat allow rules. I wrote up how I worked around that: [Isolating a Windows host when block rules always win](/writeups/windows-firewall-isolation-allowlist).

## Bugs worth remembering

The latest round fixed two bugs that had quietly disabled Tier 1 detection entirely:

- **The rules path was missing a separator.** `concat!(env!("CARGO_MANIFEST_DIR"), "../rules/examples")` produces `.../agent../rules/examples`, a directory that doesn't exist. The agent printed a warning and carried on with detection off.
- **The example rule used `#` comments.** YARA only accepts `//` and `/* */`, so the rule file failed to compile.

Both were silent in the sense that mattered: the agent still ran and still printed events. There's now a test that compiles every rule in the repo's `rules/examples/` directory, so a broken rule fails `cargo test` instead of turning detection off at runtime.

## What's next

- Verify the ETW field names against a live trace. ETW often reports `ImageName` as a kernel device path (`\Device\HarddiskVolume3\...`) rather than `C:\...`, which would make the YARA scan fail to open the file.
- Linux collector on eBPF via Aya.
- `store`: a durable local event log that survives offline periods.
- Swap the `netsh` isolation for the Windows Filtering Platform for finer control.
- Tier 2: the process-lineage behavior graph.

## Building

```bash
cd agent
cargo build
cargo test
# the ETW collector needs an elevated shell
cargo run              # alert-only
cargo run -- --enforce # kill on rules marked action = "kill"
```

To see a detection without real malware, the repo ships a synthetic rule that matches the string `POSITRON_TEST_MARKER`. Put that string in any executable and run it.
