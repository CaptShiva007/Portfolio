
use axum::{http::header, response::IntoResponse, Json};
use serde::Serialize;
use serde_json::Value;
use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
 
#[derive(Clone, Default, Serialize)]
struct Now {
    enabled: bool,
    playing: bool,
    title: String,
    artist: String,
    album: String,
    art: String,
    url: String,
    progress_ms: u64,
    duration_ms: u64,
}
 
struct Config {
    spotify: Option<(String, String, String)>, 
    lastfm: Option<(String, String)>,          
    token_url: String,
    api_url: String,
    lfm_url: String,
}
 
struct Cache {
    at: Option<Instant>,
    now: Now,
    token: Option<(String, Instant)>,
    backoff_until: Option<Instant>,
}
 
static CFG: OnceLock<Config> = OnceLock::new();
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
 
fn var(k: &str) -> Option<String> {
    std::env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}
 
fn cfg() -> &'static Config {
    CFG.get_or_init(|| Config {
        spotify: match (var("SPOTIFY_CLIENT_ID"), var("SPOTIFY_CLIENT_SECRET"), var("SPOTIFY_REFRESH_TOKEN")) {
            (Some(a), Some(b), Some(c)) => Some((a, b, c)),
            _ => None,
        },
        lastfm: match (var("LASTFM_API_KEY"), var("LASTFM_USER")) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => None,
        },
        token_url: var("SPOTIFY_TOKEN_URL").unwrap_or_else(|| "https://accounts.spotify.com/api/token".into()),
        api_url: var("SPOTIFY_API_URL").unwrap_or_else(|| "https://api.spotify.com".into()),
        lfm_url: var("LASTFM_API_URL").unwrap_or_else(|| "https://ws.audioscrobbler.com".into()),
    })
}
 
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(6)).build()
}
 
fn idle() -> Now {
    Now { enabled: true, ..Default::default() }
}
 
fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::new();
    for ch in data.chunks(3) {
        let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
        o.push(T[(n >> 18 & 63) as usize] as char);
        o.push(T[(n >> 12 & 63) as usize] as char);
        o.push(if ch.len() > 1 { T[(n >> 6 & 63) as usize] as char } else { '=' });
        o.push(if ch.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    o
}
 
// Error text without the URL: Last.fm requests carry the API key in the query string.
fn describe(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => format!("http {code}"),
        ureq::Error::Transport(t) => format!("network error ({:?})", t.kind()),
    }
}
 
fn json_of(resp: ureq::Response) -> Result<Value, String> {
    let body = resp.into_string().map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}
 
fn spotify_token(c: &Config, cache: &mut Cache, id: &str, secret: &str, refresh: &str) -> Result<String, String> {
    if let Some((t, exp)) = &cache.token {
        if Instant::now() + Duration::from_secs(30) < *exp {
            return Ok(t.clone());
        }
    }
    let basic = b64(format!("{id}:{secret}").as_bytes());
    let resp = agent()
        .post(&c.token_url)
        .set("Authorization", &format!("Basic {basic}"))
        .send_form(&[("grant_type", "refresh_token"), ("refresh_token", refresh)])
        .map_err(|e| format!("token request failed: {}", describe(&e)))?;
    let v = json_of(resp)?;
    let tok = v["access_token"].as_str().ok_or("no access_token in reply")?.to_string();
    let secs = v["expires_in"].as_u64().unwrap_or(3600);
    cache.token = Some((tok.clone(), Instant::now() + Duration::from_secs(secs)));
    Ok(tok)
}
 
fn spotify_now(c: &Config, cache: &mut Cache, id: &str, secret: &str, refresh: &str) -> Result<Now, String> {
    let tok = spotify_token(c, cache, id, secret, refresh)?;
    let r = agent()
        .get(&format!("{}/v1/me/player/currently-playing", c.api_url))
        .set("Authorization", &format!("Bearer {tok}"))
        .call();
    let resp = match r {
        Ok(resp) => resp,
        Err(ureq::Error::Status(401, _)) => {
            cache.token = None; 
            return Err("access token rejected (401)".into());
        }
        Err(ureq::Error::Status(429, resp)) => {
            let s = resp.header("Retry-After").and_then(|h| h.parse().ok()).unwrap_or(30);
            cache.backoff_until = Some(Instant::now() + Duration::from_secs(s));
            return Err(format!("rate limited, backing off {s}s"));
        }
        Err(e) => return Err(format!("currently-playing failed: {}", describe(&e))),
    };
    if resp.status() == 204 {
        return Ok(idle()); 
    }
    let v = json_of(resp)?;
    let item = &v["item"];
    if item.is_null() || v["is_playing"].as_bool() != Some(true) {
        return Ok(idle());
    }
    let images = item["album"]["images"].as_array().cloned().unwrap_or_default();
    let art = images.get(1).or_else(|| images.first()).and_then(|i| i["url"].as_str()).unwrap_or("");
    let artists: Vec<&str> = item["artists"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str()).collect()).unwrap_or_default();
    Ok(Now {
        enabled: true,
        playing: true,
        title: item["name"].as_str().unwrap_or("").to_string(),
        artist: if artists.is_empty() { item["show"]["name"].as_str().unwrap_or("").to_string() } else { artists.join(", ") },
        album: item["album"]["name"].as_str().unwrap_or("").to_string(),
        art: art.to_string(),
        url: item["external_urls"]["spotify"].as_str().unwrap_or("").to_string(),
        progress_ms: v["progress_ms"].as_u64().unwrap_or(0),
        duration_ms: item["duration_ms"].as_u64().unwrap_or(0),
    })
}
 
fn lastfm_now(c: &Config, key: &str, user: &str) -> Result<Now, String> {
    let resp = agent()
        .get(&format!("{}/2.0/", c.lfm_url))
        .query("method", "user.getrecenttracks")
        .query("user", user)
        .query("api_key", key)
        .query("format", "json")
        .query("limit", "1")
        .call()
        .map_err(|e| format!("last.fm request failed: {}", describe(&e)))?;
    let v = json_of(resp)?;
    let t = &v["recenttracks"]["track"];
    let t = if t.is_array() { &t[0] } else { t };
    if t.is_null() || t["@attr"]["nowplaying"].as_str() != Some("true") {
        return Ok(idle());
    }
    let imgs = t["image"].as_array().cloned().unwrap_or_default();
    let art = imgs.get(2).or_else(|| imgs.last()).and_then(|i| i["#text"].as_str()).unwrap_or("");
    Ok(Now {
        enabled: true,
        playing: true,
        title: t["name"].as_str().unwrap_or("").to_string(),
        artist: t["artist"]["#text"].as_str().unwrap_or("").to_string(),
        album: t["album"]["#text"].as_str().unwrap_or("").to_string(),
        art: art.to_string(),
        url: t["url"].as_str().unwrap_or("").to_string(),
        progress_ms: 0,
        duration_ms: 0,
    })
}
 
/// Cached for 4 seconds 
fn get_now() -> Now {
    let c = cfg();
    if c.spotify.is_none() && c.lastfm.is_none() {
        return Now::default(); 
    }
    let m = CACHE.get_or_init(|| Mutex::new(Cache { at: None, now: idle(), token: None, backoff_until: None }));
    let mut cache = m.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(t) = cache.at {
        if t.elapsed() < Duration::from_secs(4) {
            return cache.now.clone();
        }
    }
    if let Some(b) = cache.backoff_until {
        if Instant::now() < b {
            return cache.now.clone();
        }
        cache.backoff_until = None;
    }
    let res = if let Some((id, secret, refresh)) = &c.spotify {
        spotify_now(c, &mut cache, id, secret, refresh)
    } else if let Some((key, user)) = &c.lastfm {
        lastfm_now(c, key, user)
    } else {
        Ok(idle())
    };
    cache.now = match res {
        Ok(n) => n,
        Err(e) => {
            eprintln!("now-playing: {e}");
            idle()
        }
    };
    cache.at = Some(Instant::now());
    cache.now.clone()
}
 
pub async fn handler() -> impl IntoResponse {
    let now = tokio::task::spawn_blocking(get_now).await.unwrap_or_default();
    ([(header::CACHE_CONTROL, "no-store")], Json(now))
}
 
pub async fn js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=300")],
        include_str!("../assets/spotify.js"),
    )
}
