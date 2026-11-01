---
title: Isolating a Windows host when block rules always win
date: 2026-11-01
tags: [windows, firewall, edr, rust, positron]
summary: Cutting a host off the network except for one IP sounds like "block all, allow one". On Windows Firewall that doesn't work, so I block everything else instead.
---

One of the response actions in [Positron](/projects/positron), my EDR agent, is **network isolation**: when a host looks compromised, cut it off from the network so whatever is on it can't spread or phone home.

There's a catch. Once a host is isolated, you still need to reach it, at least from the management plane. Otherwise the only way to undo the isolation is to walk over to the machine. So the requirement is really: **block all traffic except to and from a short allowlist.**

## The obvious approach, and why it fails

On most firewalls you'd write two rules:

1. Block all traffic.
2. Allow traffic to the management server.

Windows Defender Firewall doesn't work like that. Its rules aren't evaluated top to bottom. A **block rule beats an allow rule** whenever both match the same traffic, whatever order you created them in (the one exception is authenticated IPsec bypass, which doesn't apply here). So "block all" also blocks the management server, and the allow rule never comes into play.

## Block the complement instead

If the allow rule can't punch a hole, make the hole in the block rule itself. Instead of blocking `0.0.0.0-255.255.255.255`, block **every address range except the allowed IPs**. The allowed addresses never match a block rule, so their traffic goes through untouched.

`netsh advfirewall` accepts a comma-separated list of addresses and ranges in `remoteip=`. With an allowlist of `10.0.0.5`, the IPv4 part of the rule becomes:

```
0.0.0.0-10.0.0.4,10.0.0.6-255.255.255.255
```

IPv6 is handled the same way.

## Computing the gaps

This boils down to: given a set of points in `[0, max]`, return the inclusive ranges that cover everything except those points. Treating IPv4 as a `u32` and IPv6 as a `u128` lets one function do both:

```rust
/// Inclusive ranges covering [0, max] minus the given points.
fn complement(mut points: Vec<u128>, max: u128) -> Vec<(u128, u128)> {
    points.sort_unstable();
    points.dedup();

    let mut ranges = Vec::new();
    let mut next: Option<u128> = Some(0); // None once we've passed `max`
    for p in points {
        if let Some(start) = next {
            if p > start {
                ranges.push((start, p - 1));
            }
        }
        next = p.checked_add(1).filter(|n| *n <= max);
    }
    if let Some(start) = next {
        ranges.push((start, max));
    }
    ranges
}
```

The edge cases are where this kind of code goes wrong:

- **Duplicate IPs** in the allowlist: `dedup()` after sorting.
- **Adjacent IPs** like `1.2.3.4` and `1.2.3.5`: the `p > start` check skips the empty range between them.
- **An allowed `0.0.0.0` or `255.255.255.255`**: no range should start before 0 or run past the end. For IPv6, `u128::MAX + 1` would overflow, so `next` is an `Option` and `checked_add` turns overflow into "nothing left".
- **An empty allowlist** falls out naturally as one range covering everything, so plain full isolation is the same code path.

Each of those has a unit test.

## Don't leave the host half-isolated

Isolation takes two rules, one inbound and one outbound. If the inbound rule goes in and the outbound one fails, the host ends up in a strange state that nobody asked for. So the outbound failure triggers a rollback:

```rust
run_netsh_add_block_rule(ISOLATION_RULE_IN, "in", &remote_ips)?;
if let Err(e) = run_netsh_add_block_rule(ISOLATION_RULE_OUT, "out", &remote_ips) {
    // Don't leave the host half-isolated.
    let _ = run_netsh_delete_rule(ISOLATION_RULE_IN);
    return Err(e);
}
```

De-isolation goes the other way. It tries to delete **both** rules even if the first delete fails, then reports every failure. My first version stopped at the first error and swallowed it, which is the worst outcome for this action: you think the host is back online, and it isn't.

## What I'd change

Shelling out to `netsh` is blunt. It's slow, its output is meant for humans, and it only reports success or failure through an exit code. The proper tool is the **Windows Filtering Platform (WFP)**. With WFP you set filter weights explicitly, so a high-weight permit really can override a lower-weight block, and the complement trick isn't needed at all. That's the plan once the rest of the response path has proven itself. For now, `netsh` with a computed complement works and fits in about 60 lines of testable Rust.
