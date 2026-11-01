mod nowplaying;

use axum::{
    extract::{Path as AxPath, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use pulldown_cmark::{html, Options, Parser};
use serde::Serialize;
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::Duration,
};

const SECTIONS: [(&str, &str, &str); 3] = [
    ("writeups", "Writeups", "Step-by-step solves and breakdowns."),
    ("notes", "Notes", "Short notes, cheat sheets and things I keep forgetting."),
    ("projects", "Projects", "Things I built, with the story behind them."),
];

// content model

#[derive(Clone, Default)]
struct Meta {
    title: String,
    date: String,
    summary: String,
    status: String,
    tags: Vec<String>,
    draft: bool,
}

#[derive(Clone)]
struct Entry {
    section: &'static str,
    slug: String,
    meta: Meta,
    html: String,
    minutes: usize,
}

struct Shared {
    dir: PathBuf,
    entries: RwLock<Vec<Entry>>,
}
type App = Arc<Shared>;

/// Split "---\nkey: value\n---\nbody" into metadata and body. Tolerates a missing header.
fn parse_front_matter(raw: &str) -> (Meta, String) {
    let text = raw.replace("\r\n", "\n");
    let mut meta = Meta::default();
    let body = if let Some(rest) = text.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            for line in rest[..end].lines() {
                if let Some((k, v)) = line.split_once(':') {
                    let v = v.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
                    match k.trim() {
                        "title" => meta.title = v,
                        "date" => meta.date = v,
                        "summary" => meta.summary = v,
                        "status" => meta.status = v,
                        "draft" => meta.draft = v == "true",
                        "tags" => {
                            meta.tags = v
                                .trim_matches(|c| c == '[' || c == ']')
                                .split(',')
                                .map(|t| t.trim().trim_matches(|c| c == '"' || c == '\'').to_lowercase())
                                .filter(|t| !t.is_empty())
                                .collect()
                        }
                        _ => {}
                    }
                }
            }
            rest[end + 4..].trim_start_matches('\n').to_string()
        } else {
            text.clone()
        }
    } else {
        text.clone()
    };
    (meta, body)
}

/// Slugs come only from file names and are restricted to [a-z0-9_-].
fn slugify(stem: &str) -> String {
    stem.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect()
}

fn render_markdown(md: &str) -> String {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;

    let mut processed = String::new();

    for line in md.lines() {
        let trimmed = line.trim();

        if let Some(url) = trimmed.strip_prefix("[embed]") {
            let url = url.trim();

            if url.starts_with("https://") || url.starts_with("http://") {
                processed.push_str(&format!(
                    r#"<iframe
                        src="{}"
                        title="Embedded PDF"
                        width="100%"
                        height="700"
                        style="border:1px solid var(--line);border-radius:8px"
                        loading="lazy"
                        referrerpolicy="no-referrer">
                    </iframe>"#,
                    esc(url)
                ));
                processed.push('\n');
                continue;
            }
        }

        processed.push_str(line);
        processed.push('\n');
    }

    let mut out = String::new();
    html::push_html(&mut out, Parser::new_ext(&processed, opts));
    out
}

fn md_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "md").unwrap_or(false))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn load_all(root: &Path) -> Vec<Entry> {
    let mut all = Vec::new();
    for (section, _, _) in SECTIONS {
        for path in md_files(&root.join(section)) {
            let Ok(raw) = fs::read_to_string(&path) else { continue };
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("untitled");
            let (mut meta, body) = parse_front_matter(&raw);
            if meta.draft {
                continue;
            }
            if meta.title.is_empty() {
                meta.title = stem.replace(['-', '_'], " ");
            }
            let minutes = (body.split_whitespace().count() / 200).max(1);
            all.push(Entry { section, slug: slugify(stem), meta, html: render_markdown(&body), minutes });
        }
    }
    // newest first (ISO dates sort as strings)
    all.sort_by(|a, b| b.meta.date.cmp(&a.meta.date).then(a.meta.title.cmp(&b.meta.title)));
    all
}

/// Cheap change detector: hash of every .md path, size and mtime.
fn fingerprint(root: &Path) -> u64 {
    let mut h = DefaultHasher::new();
    for (section, _, _) in SECTIONS {
        for p in md_files(&root.join(section)) {
            p.hash(&mut h);
            if let Ok(m) = fs::metadata(&p) {
                m.len().hash(&mut h);
                m.modified().ok().hash(&mut h);
            }
        }
    }
    h.finish()
}

// ---------------------------------------------------------------- html helpers

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn section_title(sec: &str) -> &'static str {
    SECTIONS.iter().find(|s| s.0 == sec).map(|s| s.1).unwrap_or("")
}

fn page(title: &str, desc: &str, active: &str, body: String) -> Html<String> {
    let nav: String = std::iter::once(("", "home"))
        .chain(SECTIONS.iter().map(|s| (s.0, s.0)))
        .map(|(href, label)| {
            let on = if href == active { " class=\"on\"" } else { "" };
            format!("<a href=\"/{href}\"{on}>{label}</a>")
        })
        .collect();
    Html(format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="google-site-verification" content="um5kZLaxoqHxV8FIfU4XRqWhfjkL5qxhX2NZEYyfj-E" />
<title>{t} // c0nqu15t4d0r</title>
<meta name="description" content="{d}">
<link rel="stylesheet" href="/static/app.css"><script src="/static/theme.js"></script></head>
<body><div class="sun"></div><div class="grid"></div><div id="progress"></div>
<header><nav><span class="ps">captshiva007@c0nqu15t4d0r:~</span>{nav}<button id="kbtn" type="button">search <kbd>ctrl+k</kbd></button><div class="ctl"><select id="ifsel" aria-label="Interface"><option value="classic">ui: classic</option><option value="terminal">ui: terminal</option><option value="desktop">ui: desktop</option></select><select id="thsel" aria-label="Theme"><option value="amber">theme: amber</option><option value="matrix">theme: matrix</option><option value="cyber">theme: cyber</option><option value="ice">theme: ice</option></select><button id="fxbtn" type="button">fx: on</button></div></nav></header>
<main>{body}</main>
<footer><span>c0nqu15t4d0r.online</span><span id="up">session 00:00:00</span></footer>
<div id="pal" hidden><div class="pbox"><input id="pin" placeholder="type to search, enter to open" autocomplete="off" aria-label="Search"><ul id="pres"></ul></div></div>
<script src="/static/app.js" defer></script><script src="/static/ui.js" defer></script><script src="/static/spotify.js" defer></script></body></html>"#,
        t = esc(title),
        d = esc(desc),
        nav = nav,
        body = body
    ))
}

fn tag_links(tags: &[String]) -> String {
    tags.iter().map(|t| format!("<a class=\"chip\" href=\"/tags/{0}\">{0}</a>", esc(&slugify(t)))).collect()
}

fn list_row(e: &Entry) -> String {
    format!(
        "<li data-q=\"{q}\" data-tags=\"{tags}\"><span class=\"dim\">{date}</span><a href=\"/{sec}/{slug}\">{title}</a><span class=\"dim\">{min} min</span></li>",
        q = esc(&format!("{} {} {}", e.meta.title, e.meta.summary, e.meta.tags.join(" ")).to_lowercase()),
        tags = esc(&e.meta.tags.join(",")),
        date = esc(&e.meta.date),
        sec = e.section,
        slug = esc(&e.slug),
        title = esc(&e.meta.title),
        min = e.minutes
    )
}

fn project_card(e: &Entry) -> String {
    let chips: String = e.meta.tags.iter().map(|t| format!("<span>{}</span>", esc(t))).collect();
    let status = if e.meta.status.is_empty() { String::new() } else { format!("<span class=\"status\"><i></i>{}</span>", esc(&e.meta.status)) };
    format!(
        "<article class=\"hud\" data-q=\"{q}\" data-tags=\"{tags}\">{status}<h3><a href=\"/{sec}/{slug}\">{title}</a></h3><p>{sum}</p><div class=\"chips\">{chips}</div></article>",
        q = esc(&format!("{} {} {}", e.meta.title, e.meta.summary, e.meta.tags.join(" ")).to_lowercase()),
        tags = esc(&e.meta.tags.join(",")),
        sec = e.section,
        slug = esc(&e.slug),
        title = esc(&e.meta.title),
        sum = esc(&e.meta.summary)
    )
}

// ---------------------------------------------------------------- handlers

async fn home(State(app): State<App>) -> Html<String> {
    let entries = app.entries.read().unwrap();
    let recent: String = entries.iter().take(6).map(list_row).collect();
    let counts: String = SECTIONS
        .iter()
        .map(|(s, name, _)| {
            let n = entries.iter().filter(|e| e.section == *s).count();
            format!("<a class=\"hud stat\" href=\"/{s}\"><b>{n}</b><span>{name}</span></a>")
        })
        .collect();
    let body = format!(
        r#"<section class="hero"><div id="boot" aria-live="polite"></div>
<h1 data-t="c0nqu15t4d0r">c0nqu15t4d0r</h1>
<p class="tag">I build small systems, break them on purpose, and write down what I learn.<span class="cur"></span></p>
<div class="stats">{counts}</div></section>
<section><h2>tail -f recent.log</h2><div class="term"><div class="bar">recent.log</div><div class="bd"><ul class="list">{recent}</ul></div></div>
<p class="dim hint">Press <kbd>ctrl+k</kbd> to search everything.</p></section>"#
    );
    page("home", "Writeups, notes and projects by captshiva007.", "", body)
}

async fn section_list(State(app): State<App>, AxPath(sec): AxPath<String>) -> Response {
    let Some((key, name, blurb)) = SECTIONS.iter().find(|s| s.0 == sec).copied() else { return not_found() };
    let entries = app.entries.read().unwrap();
    let items: Vec<&Entry> = entries.iter().filter(|e| e.section == key).collect();
    let mut tags: Vec<String> = items.iter().flat_map(|e| e.meta.tags.clone()).collect();
    tags.sort();
    tags.dedup();
    let chips: String = tags.iter().map(|t| format!("<button type=\"button\" class=\"chip\" data-tag=\"{0}\">{0}</button>", esc(t))).collect();
    let listing = if key == "projects" {
        format!("<div class=\"grid2\" id=\"items\">{}</div>", items.iter().map(|e| project_card(e)).collect::<String>())
    } else {
        format!("<div class=\"term\"><div class=\"bd\"><ul class=\"list\" id=\"items\">{}</ul></div></div>", items.iter().map(|e| list_row(e)).collect::<String>())
    };
    let empty = if items.is_empty() { format!("<p class=\"dim\">Nothing here yet. Add a .md file to content/{key}/.</p>") } else { String::new() };
    let body = format!(
        "<section class=\"first\"><h2>ls {key}/</h2><p class=\"dim\">{blurb}</p><div class=\"tools\"><input id=\"filter\" placeholder=\"filter {key}...\" aria-label=\"Filter\"><div class=\"chips\">{chips}</div></div>{listing}{empty}</section>",
        blurb = esc(blurb)
    );
    page(name, blurb, key, body).into_response()
}

async fn entry_page(State(app): State<App>, AxPath((sec, slug)): AxPath<(String, String)>) -> Response {
    let entries = app.entries.read().unwrap();
    let Some(e) = entries.iter().find(|e| e.section == sec && e.slug == slug) else { return not_found() };
    let body = format!(
        "<section class=\"first\"><p class=\"crumb\"><a href=\"/{sec}\">{secname}</a> / {slug}</p><article class=\"layout\"><div class=\"prose\" id=\"prose\"><h1 class=\"ptitle\">{title}</h1><p class=\"meta dim\">{date} &middot; {min} min read</p><div class=\"chips\">{tags}</div>{html}</div><aside id=\"toc\" aria-label=\"Table of contents\"></aside></article></section>",
        sec = e.section,
        secname = section_title(e.section),
        slug = esc(&e.slug),
        title = esc(&e.meta.title),
        date = esc(&e.meta.date),
        min = e.minutes,
        tags = tag_links(&e.meta.tags),
        html = e.html
    );
    page(&e.meta.title, &e.meta.summary, e.section, body).into_response()
}

async fn tag_page(State(app): State<App>, AxPath(tag): AxPath<String>) -> Response {
    let entries = app.entries.read().unwrap();
    let tag = slugify(&tag);
    let rows: String = entries.iter().filter(|e| e.meta.tags.iter().any(|t| slugify(t) == tag)).map(list_row).collect();
    let body = format!(
        "<section class=\"first\"><h2>grep -r {0} .</h2><div class=\"term\"><div class=\"bd\"><ul class=\"list\">{1}</ul></div></div></section>",
        esc(&tag),
        if rows.is_empty() { "<li class=\"dim\">No matches.</li>".to_string() } else { rows }
    );
    page(&format!("tag: {tag}"), "Tagged entries", "", body).into_response()
}

#[derive(Serialize)]
struct SearchItem {
    t: String,
    s: String,
    u: String,
    k: &'static str,
    g: Vec<String>,
}

async fn search_json(State(app): State<App>) -> Json<Vec<SearchItem>> {
    let entries = app.entries.read().unwrap();
    Json(
        entries
            .iter()
            .map(|e| SearchItem {
                t: e.meta.title.clone(),
                s: e.meta.summary.clone(),
                u: format!("/{}/{}", e.section, e.slug),
                k: e.section,
                g: e.meta.tags.clone(),
            })
            .collect(),
    )
}

fn not_found() -> Response {
    let body = "<section class=\"hero\"><h1 data-t=\"404\">404</h1><p class=\"tag\">No such file or directory. Try <a href=\"/\">home</a> or press <kbd>ctrl+k</kbd>.</p></section>".to_string();
    (StatusCode::NOT_FOUND, page("not found", "Page not found", "", body)).into_response()
}

async fn fallback() -> Response {
    not_found()
}

async fn css() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=300")], include_str!("../assets/app.css"))
}

async fn js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=300")], include_str!("../assets/app.js"))
}

async fn theme_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=300")], include_str!("../assets/theme.js"))
}

async fn ui_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=300")], include_str!("../assets/ui.js"))
}

async fn serve_asset(AxPath(path): AxPath<String>) -> Response {
    use std::path::Component;

    let relative = Path::new(&path);

    /*  Reject paths that could escape the assets directory. */
    if relative.components().any(|c| !matches!(c, Component::Normal(_))) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let file = Path::new("assets").join(relative);

    let content_type = match file.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("avif") => "image/avif",
        _ => return StatusCode::NOT_FOUND.into_response(),
    };

    match fs::read(&file) {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, content_type),
                (header::CACHE_CONTROL, "public, max-age=3600"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn healthz() -> &'static str {
    "ok"
}

async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    h.insert("referrer-policy", HeaderValue::from_static("strict-origin-when-cross-origin"));
    h.insert("x-frame-options", HeaderValue::from_static("DENY"));
    h.insert(
        "content-security-policy",
        HeaderValue::from_static("default-src 'self'; img-src 'self' data: https:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"),
    );
    res
}

// ---------------------------------------------------------------- main

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}

#[tokio::main]
async fn main() {
    let dir = PathBuf::from(std::env::var("CONTENT_DIR").unwrap_or_else(|_| "content".into()));
    let bind: SocketAddr = std::env::var("BIND")
        .unwrap_or_else(|_| "127.0.0.1:3000".into())
        .parse()
        .expect("BIND must look like 0.0.0.0:3000");

    let app: App = Arc::new(Shared { dir: dir.clone(), entries: RwLock::new(load_all(&dir)) });
    println!("loaded {} entries from {}", app.entries.read().unwrap().len(), dir.display());

    // live reload: rescan when any .md file changes
    let watcher = app.clone();
    tokio::spawn(async move {
        let mut last = fingerprint(&watcher.dir);
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let now = fingerprint(&watcher.dir);
            if now != last {
                last = now;
                let fresh = load_all(&watcher.dir);
                println!("content changed: {} entries", fresh.len());
                *watcher.entries.write().unwrap() = fresh;
            }
        }
    });

    let router = Router::new()
        .route("/", get(home))
        .route("/healthz", get(healthz))
        .route("/search.json", get(search_json))
        .route("/static/app.css", get(css))
        .route("/static/app.js", get(js))
        .route("/static/theme.js", get(theme_js))
        .route("/assets/*path", get(serve_asset))
        .route("/static/ui.js", get(ui_js))
        .route("/static/spotify.js", get(nowplaying::js))
        .route("/api/now-playing", get(nowplaying::handler))
        .route("/tags/:tag", get(tag_page))
        .route("/:section", get(section_list))
        .route("/:section/:slug", get(entry_page))
        .fallback(fallback)
        .layer(middleware::from_fn(security_headers))
        .with_state(app);

    let listener = tokio::net::TcpListener::bind(bind).await.expect("cannot bind");
    println!("listening on http://{bind}");
    axum::serve(listener, router).with_graceful_shutdown(shutdown_signal()).await.unwrap();
}
