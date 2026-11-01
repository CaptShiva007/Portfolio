# prism-site

A Markdown-driven website server in Rust (axum + pulldown-cmark).

## Write content
Add `.md` files to `content/writeups`, `content/notes` or `content/projects`.
They show up on the site within ~2 seconds. No rebuild, no restart.

```
---
title: My title
date: 2026-10-05
tags: [ctf, web]
summary: One line shown in lists and search.
status: live        # projects only
draft: true         # optional: hides the file
---
Markdown here.
```

## Run locally
    cargo run                      # http://127.0.0.1:3000
    BIND=0.0.0.0:8000 CONTENT_DIR=./content cargo run --release

## Features
- Ctrl+K or `/` opens a search palette over everything
- Tag chips and a live filter on each section page
- Table of contents, reading progress bar, copy buttons on code blocks
- Strict Content-Security-Policy and escaping of all front-matter text
