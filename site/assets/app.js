(function () {
  "use strict";
  var $ = function (s, r) { return (r || document).querySelector(s); };
  var $$ = function (s, r) { return [].slice.call((r || document).querySelectorAll(s)); };
  var reduce = matchMedia("(prefers-reduced-motion:reduce)").matches;

  // session timer, doesn't survive refresh tho
  var t0 = Date.now(), up = $("#up");
  function pad(n) { return String(n).padStart(2, "0"); }
  setInterval(function () {
    var s = Math.floor((Date.now() - t0) / 1000);
    up.textContent = "session " + pad(Math.floor(s / 3600)) + ":" + pad(Math.floor(s / 60) % 60) + ":" + pad(s % 60);
  }, 1000);

  // bullshi boot sequence
  var boot = $("#boot");
  if (boot) {
    var lines = ["> booting c0nqu15t4d0r.online ...", "> network: ipv6 [ ok ]", "> content: markdown index [ ok ]", "> server: rust [ ok ]", "> access granted"];
    var i = 0;
    (function next() {
      if (i >= lines.length) return;
      var d = document.createElement("div");
      d.textContent = lines[i++];
      boot.appendChild(d);
      setTimeout(next, reduce ? 0 : 380);
    })();
  }

  // reading progress
  var bar = $("#progress");
  function prog() {
    var h = document.documentElement;
    var max = h.scrollHeight - h.clientHeight;
    bar.style.width = (max > 0 ? (h.scrollTop / max) * 100 : 0) + "%";
  }
  addEventListener("scroll", prog, { passive: true });
  prog();

  // code copy buttons + table of contents
  var prose = $("#prose");
  if (prose) {
    $$("pre", prose).forEach(function (pre) {
      var b = document.createElement("button");
      b.type = "button"; b.className = "copy"; b.textContent = "copy";
      b.addEventListener("click", function () {
        var code = pre.querySelector("code");
        navigator.clipboard.writeText((code || pre).innerText).then(function () {
          b.textContent = "copied";
          setTimeout(function () { b.textContent = "copy"; }, 1400);
        });
      });
      pre.appendChild(b);
    });

    var toc = $("#toc"), heads = $$("h2,h3", prose), used = {};
    heads.forEach(function (h) {
      var id = h.textContent.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "section";
      if (used[id]) id += "-" + (++used[id]); else used[id] = 1;
      h.id = id;
      var a = document.createElement("a");
      a.href = "#" + id; a.textContent = h.textContent;
      a.className = h.tagName === "H3" ? "l3" : "";
      toc.appendChild(a);
    });
    if (heads.length) {
      var links = $$("a", toc);
      var spy = new IntersectionObserver(function (es) {
        es.forEach(function (e) {
          if (e.isIntersecting) links.forEach(function (a) { a.classList.toggle("on", a.getAttribute("href") === "#" + e.target.id); });
        });
      }, { rootMargin: "-20% 0px -70% 0px" });
      heads.forEach(function (h) { spy.observe(h); });
    }
  }

  // filter: text box + tag chips
  var items = $("#items"), filter = $("#filter");
  if (items && filter) {
    var active = {};
    var rows = $$("[data-q]", items);
    var apply = function () {
      var q = filter.value.toLowerCase().trim(), tags = Object.keys(active);
      rows.forEach(function (r) {
        var rt = (r.dataset.tags || "").split(",");
        var okq = !q || r.dataset.q.indexOf(q) > -1;
        var okt = tags.every(function (t) { return rt.indexOf(t) > -1; });
        r.hidden = !(okq && okt);
      });
    };
    filter.addEventListener("input", apply);
    $$(".chip[data-tag]").forEach(function (c) {
      c.addEventListener("click", function () {
        var t = c.dataset.tag;
        if (active[t]) { delete active[t]; c.classList.remove("on"); } else { active[t] = 1; c.classList.add("on"); }
        apply();
      });
    });
  }

  // (ctrl+k or "/")
  var pal = $("#pal"), pin = $("#pin"), pres = $("#pres"), data = null, sel = 0, shown = [];
  var go = [
    { t: "Home", u: "/", k: "go" }, { t: "Writeups", u: "/writeups", k: "go" },
    { t: "Notes", u: "/notes", k: "go" }, { t: "Projects", u: "/projects", k: "go" }
  ];
  function render() {
    pres.textContent = "";
    shown.forEach(function (it, n) {
      var li = document.createElement("li");
      if (n === sel) li.className = "sel";
      var a = document.createElement("span"); a.textContent = it.t;
      var b = document.createElement("small"); b.textContent = it.k;
      li.appendChild(a); li.appendChild(b);
      li.addEventListener("mousedown", function (e) { e.preventDefault(); location.href = it.u; });
      pres.appendChild(li);
    });
  }
  function search() {
    var q = pin.value.toLowerCase().trim().split(/\s+/).filter(Boolean);
    var pool = go.concat(data || []);
    shown = pool.filter(function (it) {
      var hay = (it.t + " " + (it.s || "") + " " + (it.g || []).join(" ") + " " + it.k).toLowerCase();
      return q.every(function (w) { return hay.indexOf(w) > -1; });
    }).slice(0, 8);
    sel = 0; render();
  }
  function open() {
    pal.hidden = false; pin.value = ""; pin.focus();
    if (!data) fetch("/search.json").then(function (r) { return r.json(); }).then(function (d) { data = d; search(); }).catch(function () { data = []; });
    search();
  }
  function close() { pal.hidden = true; }
  $("#kbtn").addEventListener("click", open);
  pal.addEventListener("mousedown", function (e) { if (e.target === pal) close(); });
  pin.addEventListener("input", search);
  pin.addEventListener("keydown", function (e) {
    if (e.key === "ArrowDown") { sel = Math.min(sel + 1, shown.length - 1); render(); e.preventDefault(); }
    else if (e.key === "ArrowUp") { sel = Math.max(sel - 1, 0); render(); e.preventDefault(); }
    else if (e.key === "Enter" && shown[sel]) location.href = shown[sel].u;
    else if (e.key === "Escape") close();
  });
  addEventListener("keydown", function (e) {
    var typing = /input|textarea/i.test((document.activeElement || {}).tagName || "");
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") { e.preventDefault(); pal.hidden ? open() : close(); }
    else if (e.key === "/" && !typing) { e.preventDefault(); open(); }
    else if (e.key === "Escape" && !pal.hidden) close();
  });
})();
