---
title: Self-hosting behind CGNAT: what actually worked
date: 2026-10-04
tags: [networking, ipv6, cloudflare]
summary: Port forwarding was dead on arrival. Here is the IPv6 route I took instead.
---

My ISP puts me behind **CGNAT**, so the usual "forward a port on the router" trick does nothing.

## The check

Compare the WAN IP on the router with what the internet sees. If they differ, you are behind CGNAT.

```powershell
ipconfig | findstr "IPv6"
```

## What worked

1. Get a global IPv6 address for the laptop.
2. Point a DuckDNS name at it.
3. Put Cloudflare in front so IPv4-only visitors can still reach it.

### The gotcha

The router's IPv6 firewall rule is tied to one exact address, so every prefix change breaks it.

> A script that fixes the rule after a power cut beats doing it by hand every time. Cause I'm too broke to buy a power backup

| Layer | Tool |
|---|---|
| DNS | DuckDNS |
| Proxy | Cloudflare |
| Web | Caddy + Docker |