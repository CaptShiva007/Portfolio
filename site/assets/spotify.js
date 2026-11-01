(function () {
  "use strict";
  var root = document.documentElement;
  var reduceMq = window.matchMedia ? matchMedia("(prefers-reduced-motion:reduce)") : { matches: false };
  var box = null, link = null, cv = null, ctx = null, modeEl = null, bits = {};
  var tick = null, timer = null, stopped = false, p0 = 0, t0 = 0, dur = 0;
  var raf = 0, shown = false, mode = "bars", dpr = 1, W = 0, H = 0;
  var colA = "#ffb000", colB = "#ff7a00", seedKey = "", voices = null, peaks = [];
  try { var saved = localStorage.getItem("npviz"); if (saved === "wave" || saved === "bars") mode = saved; } catch (e) {}

  function el(tag, cls) { var n = document.createElement(tag); if (cls) n.className = cls; return n; }
  function safe(u, re) { return typeof u === "string" && re.test(u) ? u : ""; }

  // ---------- visualizers (animated, seeded by the song so each track has its own look) ----------
  function seedOf(s) { var h = 2166136261; for (var i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); } return h >>> 0; }
  function rng(a) { return function () { a |= 0; a = a + 0x6D2B79F5 | 0; var t = Math.imul(a ^ a >>> 15, 1 | a); t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t; return ((t ^ t >>> 14) >>> 0) / 4294967296; }; }
  function initVoices(key) {
    var r = rng(seedOf(key)), N = 28, i;
    voices = { bpm: 84 + r() * 70, bars: [], waves: [] };
    for (i = 0; i < N; i++) voices.bars.push({ f1: 0.6 + r() * 2.2, f2: 0.3 + r() * 1.6, p1: r() * 6.28, p2: r() * 6.28 });
    for (i = 0; i < 3; i++) voices.waves.push({ f: 1.5 + r() * 4 + i * 3.5, p: r() * 6.28, a: 0.5 / (i + 1), s: 0.6 + r() * 1.4 });
    peaks = []; for (i = 0; i < N; i++) peaks.push(0);
  }
  function colors() {
    var cs = getComputedStyle(root);
    colA = cs.getPropertyValue("--a").trim() || colA;
    colB = cs.getPropertyValue("--a2").trim() || colB;
  }
  function size() {
    if (!cv) return;
    dpr = Math.min(2, window.devicePixelRatio || 1);
    W = cv.width = Math.round((cv.clientWidth || 280) * dpr);
    H = cv.height = Math.round((cv.clientHeight || 34) * dpr);
  }

  function drawBars(t, beat) {
    var n = voices.bars.length, gap = 2 * dpr, bw = (W - (n - 1) * gap) / n, i;
    ctx.shadowColor = colA; ctx.shadowBlur = 6 * dpr;
    for (i = 0; i < n; i++) {
      var b = voices.bars[i], tilt = 1 - (i / n) * 0.55;               // lows louder than highs
      var v = Math.abs(Math.sin(t * b.f1 + b.p1) * Math.sin(t * b.f2 + b.p2));
      var h = Math.min(1, 0.08 + v * 0.8 * tilt + beat * 0.22 * tilt);
      peaks[i] = Math.max(h, peaks[i] - 0.012);                         // peak caps fall slowly
      var bh = Math.max(2 * dpr, h * (H - 2 * dpr)), x = i * (bw + gap);
      ctx.fillStyle = colA; ctx.fillRect(x, H - bh, bw, bh);
      ctx.fillStyle = colB; ctx.fillRect(x, H - Math.max(2 * dpr, peaks[i] * (H - 2 * dpr)) - 2 * dpr, bw, 2 * dpr);
    }
  }
  function trace(t, beat, col, alpha, blur) {
    var mid = H / 2, x, k;
    ctx.globalAlpha = alpha; ctx.strokeStyle = col; ctx.shadowColor = col; ctx.shadowBlur = blur;
    ctx.beginPath();
    for (x = 0; x <= W; x += 2 * dpr) {
      var u = x / W, y = 0;
      for (k = 0; k < voices.waves.length; k++) { var w = voices.waves[k]; y += w.a * Math.sin(u * w.f * 6.283 + t * w.s * 6 + w.p); }
      var amp = (0.45 + beat * 0.55) * Math.sin(u * Math.PI);          // taper to the edges, swell on the beat
      y = Math.max(-1, Math.min(1, y * 1.5));
      if (x === 0) ctx.moveTo(x, mid); else ctx.lineTo(x, mid - y * amp * (mid - 2 * dpr));
    }
    ctx.stroke();
  }
  function drawWave(t, beat) {
    ctx.lineWidth = 1.5 * dpr;
    ctx.globalAlpha = 0.2; ctx.strokeStyle = colA; ctx.shadowBlur = 0; ctx.setLineDash([3 * dpr, 5 * dpr]);
    ctx.beginPath(); ctx.moveTo(0, H / 2); ctx.lineTo(W, H / 2); ctx.stroke(); ctx.setLineDash([]);
    trace(t - 0.07, beat, colB, 0.4, 0);                                // ghost trace
    trace(t, beat, colA, 1, 8 * dpr);                                   // main trace
  }
  function draw(t) {
    if (!ctx || !voices) return;
    ctx.clearRect(0, 0, W, H); ctx.globalAlpha = 1; ctx.shadowBlur = 0;
    var beat = Math.pow(Math.max(0, Math.sin(t * voices.bpm / 60 * Math.PI * 2)), 5);
    if (mode === "bars") drawBars(t, beat); else drawWave(t, beat);
    ctx.globalAlpha = 1; ctx.shadowBlur = 0;
  }
  function animating() { return root.dataset.fx !== "off" && !reduceMq.matches; }
  function frame(ts) { raf = 0; if (!shown || document.hidden) return; draw(ts / 1000); if (animating()) raf = requestAnimationFrame(frame); }
  function kick() { if (!raf && shown && !document.hidden) raf = requestAnimationFrame(frame); }

  function toggleMode() {
    mode = mode === "bars" ? "wave" : "bars";
    try { localStorage.setItem("npviz", mode); } catch (e) {}
    modeEl.textContent = mode; cv.parentNode.setAttribute("aria-label", "Visualizer: " + mode + ". Click to switch.");
    kick();
  }

  // ---------- card ----------
  function build() {
    box = el("div", "hud"); box.id = "np"; box.hidden = true;
    link = el("a", "nplink"); link.target = "_blank"; link.rel = "noopener noreferrer";
    var disc = el("div", "npdisc"), img = el("img"); img.alt = ""; img.width = 56; img.height = 56; disc.appendChild(img);
    var txt = el("div", "nptxt"), ti = el("div", "nptitle"), ar = el("div", "npartist"), bar = el("div", "npbar"), fill = el("i");
    bar.appendChild(fill); txt.appendChild(ti); txt.appendChild(ar); txt.appendChild(bar);
    link.appendChild(disc); link.appendChild(txt);
    var viz = el("button", "npviz"); viz.type = "button"; viz.title = "Switch visualizer";
    cv = el("canvas"); modeEl = el("span", "npmode"); modeEl.textContent = mode;
    viz.setAttribute("aria-label", "Visualizer: " + mode + ". Click to switch.");
    viz.appendChild(cv); viz.appendChild(modeEl);
    viz.addEventListener("click", toggleMode);
    box.appendChild(link); box.appendChild(viz);
    document.body.appendChild(box);
    ctx = cv.getContext("2d");
    bits = { img: img, ti: ti, ar: ar, bar: bar, fill: fill };
    new MutationObserver(function () { colors(); kick(); }).observe(root, { attributes: true, attributeFilter: ["data-theme", "data-fx"] });
    addEventListener("resize", function () { size(); kick(); });
    colors();
  }

  function paint() {
    if (!dur) return;
    var p = Math.min(dur, p0 + (Date.now() - t0));
    bits.fill.style.width = (p / dur) * 100 + "%";
  }

  function update(d) {
    if (!d || !d.enabled) { stopped = true; clearInterval(timer); shown = false; if (box) box.hidden = true; return; }
    if (!d.playing) { shown = false; if (box) box.hidden = true; clearInterval(tick); return; }
    if (!box) build();
    var art = safe(d.art, /^https:\/\//);
    if (art && bits.img.getAttribute("src") !== art) bits.img.src = art;
    bits.img.style.visibility = art ? "visible" : "hidden";
    bits.ti.textContent = d.title; bits.ar.textContent = d.artist;
    link.title = d.title + " - " + d.artist + (d.album ? " (" + d.album + ")" : "");
    var href = safe(d.url, /^https:\/\/(open\.spotify\.com|www\.last\.fm)\//);
    if (href) link.href = href; else link.removeAttribute("href");
    p0 = d.progress_ms || 0; dur = d.duration_ms || 0; t0 = Date.now();
    bits.bar.style.display = dur ? "block" : "none";
    clearInterval(tick); if (dur) { paint(); tick = setInterval(paint, 500); }
    var key = d.title + "|" + d.artist;
    if (key !== seedKey) {
      seedKey = key; initVoices(key);
      box.classList.remove("npnew"); void box.offsetWidth; box.classList.add("npnew");
    }
    box.hidden = false; shown = true; size(); kick();
  }

  function poll() {
    if (stopped || document.hidden) return;
    fetch("/api/now-playing", { cache: "no-store" })
      .then(function (r) { return r.json(); })
      .then(update)
      .catch(function () {});
  }
  poll();
  timer = setInterval(poll, 5000);
  document.addEventListener("visibilitychange", function () { poll(); kick(); });
})();