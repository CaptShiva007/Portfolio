---
title: Docker Compose cheat sheet
date: 2026-10-02
tags: [docker, cheatsheet]
summary: The handful of commands I actually use every week.
---

## Everyday

```bash
docker compose up -d
docker compose ps
docker compose logs -f --tail 50 seafile
docker compose restart caddy
```

## Careful with these

- `docker compose down` stops and removes containers.
- Adding `-v` also deletes the volumes, which means your data.
