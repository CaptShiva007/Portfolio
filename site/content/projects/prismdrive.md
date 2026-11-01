---
title: PrismDrive
date: 2026-10-03
status: live
tags: [seafile, docker, caddy]
summary: A personal cloud drive on my own laptop, served through Cloudflare.
---

PrismDrive is a **Seafile** server running in Docker, with Caddy routing by hostname.

## Why

I wanted a drive I control, reachable from anywhere, on a student budget.

## Stack

- Seafile + MariaDB + Redis
- Caddy as the front door
- Cloudflare for DNS and HTTPS
