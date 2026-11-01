(function () {
  "use strict";
  var $ = function (s, r) { return (r || document).querySelector(s); };
  var $$ = function (s, r) { return [].slice.call((r || document).querySelectorAll(s)); };
  var root = document.documentElement;
  var THEMES = ["amber", "matrix", "cyber", "ice"], UIS = ["classic", "terminal", "desktop"];
  var thsel = $("#thsel"), ifsel = $("#ifsel"), fxb = $("#fxbtn"), prose = $("#prose"), dataP = null;

  function save(k, v) { try { localStorage.setItem(k, v); } catch (e) {} }
  function el(tag, cls, txt) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (txt != null) n.textContent = txt;
    return n;
  }
  function cycle(list, cur) { return list[(list.indexOf(cur) + 1) % list.length]; }
  function slug(it) { return it.u.split("/")[2]; }
  function data() {
    return dataP || (dataP = fetch("/search.json").then(function (r) { return r.json(); }).catch(function () { return []; }));
  }
  function fit(n) { n.style.top = $("header").offsetHeight + "px"; }

  // theme / effects / reader 
  function setTheme(t) { if (THEMES.indexOf(t) < 0) return; root.dataset.theme = t; thsel.value = t; save("theme", t); }
  function setFx(on) { root.dataset.fx = on ? "on" : "off"; fxb.textContent = "fx: " + (on ? "on" : "off"); save("fx", root.dataset.fx); }

  var rd = null;
  function setReader(on) {
    if (!prose) return;
    root.dataset.reader = on ? "on" : "off";
    rd.textContent = on ? "exit reader" : "reader";
  }
  if (prose) {
    rd = el("button", null, "reader"); rd.type = "button"; rd.id = "rdbtn";
    $("#kbtn").before(rd);
    rd.addEventListener("click", function () { setReader(root.dataset.reader !== "on"); });
  }

  //  interfaces  
  var termEl = null, deskEl = null;
  function setUi(u) {
    if (UIS.indexOf(u) < 0) u = "classic";
    [termEl, deskEl].forEach(function (n) { if (n) n.remove(); });
    termEl = deskEl = null;
    root.dataset.ui = u; ifsel.value = u; save("ui", u);
    if (u === "terminal") mountTerm(); else if (u === "desktop") mountDesk();
  }

  // terminal - some additions to be done like tab-completions
  function mountTerm() {
    termEl = el("div"); termEl.id = "term"; fit(termEl);
    var out = el("div"); out.id = "tout";
    var row = el("div", "trow"), ps = el("span"), inp = el("input");
    ps.id = "tps"; inp.id = "tin"; inp.autocomplete = "off"; inp.spellcheck = false;
    inp.setAttribute("aria-label", "Terminal input");
    row.appendChild(ps); row.appendChild(inp);
    termEl.appendChild(out); termEl.appendChild(row);
    document.body.appendChild(termEl);

    var cwd = "", hist = [], hi = 0;
    function prompt() { ps.textContent = "shiva@c0nqu15t4d0r:~" + (cwd ? "/" + cwd : "") + "$"; }
    function say(t, href) {
      var d = el("div");
      if (href) { var a = el("a", null, t); a.href = href; d.appendChild(a); } else d.textContent = t;
      out.appendChild(d); out.scrollTop = out.scrollHeight;
    }
    function find(list, arg) {
      arg = (arg || "").replace(/^\.?\//, "").replace(/\/$/, "");
      return list.filter(function (i) { return slug(i) === arg || i.u === "/" + arg || i.u === "/" + cwd + "/" + arg; })[0];
    }
    var cmds = {
      help: function () {
        ["help               show this", "ls [section]        list sections or entries", "cd <section> | ..  change directory",
          "cat <name>          details of an entry", "open <name>         open the page", "search <words>      search everything",
          "theme [name]        amber | matrix | cyber | ice", "fx on|off           visual effects", "clear              clear the screen",
          "exit                back to the normal site"].forEach(function (l) { say(l); });
      },
      ls: function (a, list) {
        var sec = a[0] ? a[0].replace(/\/$/, "") : cwd;
        if (!sec) { ["writeups/", "notes/", "projects/"].forEach(function (s) { say(s); }); return; }
        var rows = list.filter(function (i) { return i.k === sec; });
        if (!rows.length) say("(empty or unknown directory)");
        rows.forEach(function (i) { say(slug(i) + "  " + i.t, i.u); });
      },
      cd: function (a) {
        var t = (a[0] || "").replace(/\/$/, "");
        if (!t || t === ".." || t === "/" || t === "~") cwd = "";
        else if (["writeups", "notes", "projects"].indexOf(t) > -1) cwd = t;
        else say("cd: no such directory: " + t);
      },
      cat: function (a, list) {
        var it = find(list, a[0]);
        if (!it) { say("cat: not found: " + (a[0] || "")); return; }
        say(it.t); say("tags: " + (it.g.join(", ") || "none")); say(it.s || "(no summary)"); say("open " + slug(it) + " to read it");
      },
      open: function (a, list) {
        var it = find(list, a[0]);
        if (it) location.href = it.u; else say("open: not found: " + (a[0] || ""));
      },
      search: function (a, list) {
        var q = a.join(" ").toLowerCase();
        var hit = list.filter(function (i) { return (i.t + " " + i.s + " " + i.g.join(" ")).toLowerCase().indexOf(q) > -1; });
        if (!q || !hit.length) say("no matches");
        else hit.forEach(function (i) { say(i.k + "/" + slug(i) + "  " + i.t, i.u); });
      },
      theme: function (a) {
        if (!a[0]) say("theme: " + root.dataset.theme + "  (available: " + THEMES.join(", ") + ")");
        else if (THEMES.indexOf(a[0]) < 0) say("unknown theme: " + a[0]); else setTheme(a[0]);
      },
      fx: function (a) { setFx(a[0] !== "off"); },
      clear: function () { out.textContent = ""; },
      whoami: function () { say("captshiva007"); },
      exit: function () { setUi("classic"); }
    };
    inp.addEventListener("keydown", function (e) {
      if (e.key === "ArrowUp") { hi = Math.max(0, hi - 1); inp.value = hist[hi] || ""; e.preventDefault(); return; }
      if (e.key === "ArrowDown") { hi = Math.min(hist.length, hi + 1); inp.value = hist[hi] || ""; e.preventDefault(); return; }
      if (e.key !== "Enter") return;
      var line = inp.value.trim(); inp.value = "";
      say(ps.textContent + " " + line);
      if (!line) return;
      hist.push(line); hi = hist.length;
      var parts = line.split(/\s+/), name = parts.shift().toLowerCase();
      if (!cmds[name]) { say("command not found: " + name + " (try help)"); return; }
      data().then(function (list) { cmds[name](parts, list); prompt(); });
    });
    termEl.addEventListener("click", function () { inp.focus(); });
    say("c0nqu15t4d0r shell. type 'help' to begin."); prompt(); inp.focus();
  }

  // desktop: some updates pending
  var zc = 10, wn = 0;
  function mountDesk() {
    deskEl = el("div"); deskEl.id = "desk"; fit(deskEl);
    ["writeups", "notes", "projects"].forEach(function (s) {
      var b = el("button", "ico", s + "/"); b.type = "button";
      b.addEventListener("click", function () { folder(s); });
      deskEl.appendChild(b);
    });
    deskEl.appendChild(el("p", "dim dhint", "Open a folder. Drag windows by their title bar."));
    document.body.appendChild(deskEl);
  }
  function win(title, node) {
    var w = el("div", "win"), bar = el("div", "wbar"), x = el("button", "wx", "x"), bd = el("div", "wbd");
    x.type = "button"; bar.appendChild(el("span", null, title)); bar.appendChild(x);
    bd.appendChild(node); w.appendChild(bar); w.appendChild(bd);
    wn++; w.style.left = 170 + (wn % 6) * 28 + "px"; w.style.top = 24 + (wn % 6) * 28 + "px"; w.style.zIndex = ++zc;
    deskEl.appendChild(w);
    w.addEventListener("pointerdown", function () { w.style.zIndex = ++zc; });
    x.addEventListener("click", function () { w.remove(); });
    bar.addEventListener("pointerdown", function (e) {
      if (e.target === x) return;
      var ox = e.clientX - w.offsetLeft, oy = e.clientY - w.offsetTop;
      bar.setPointerCapture(e.pointerId);
      function mv(ev) { w.style.left = Math.max(0, ev.clientX - ox) + "px"; w.style.top = Math.max(0, ev.clientY - oy) + "px"; }
      function up() { bar.removeEventListener("pointermove", mv); bar.removeEventListener("pointerup", up); }
      bar.addEventListener("pointermove", mv); bar.addEventListener("pointerup", up);
    });
  }
  function folder(sec) {
    data().then(function (list) {
      var box = el("div");
      var rows = list.filter(function (i) { return i.k === sec; });
      if (!rows.length) box.appendChild(el("p", "dim", "Empty folder."));
      rows.forEach(function (i) {
        var a = el("a", "wrow", i.t); a.href = i.u;
        a.addEventListener("click", function (e) { e.preventDefault(); article(i); });
        box.appendChild(a);
      });
      win(sec + "/", box);
    });
  }
  function article(i) {
    fetch(i.u).then(function (r) { return r.text(); }).then(function (h) {
      var p = new DOMParser().parseFromString(h, "text/html").querySelector("#prose");
      if (!p) return;
      p = document.importNode(p, true); p.removeAttribute("id");
      win(i.t, p);
    });
  }

  // keyboard shortcuts - Some updates can be done 
  var help = null, pendingG = false, cur = -1;
  function toggleHelp() {
    if (help) { help.remove(); help = null; return; }
    help = el("div"); help.id = "help";
    var box = el("div", "pbox hbox");
    ["Keyboard shortcuts", "", "ctrl+k or /      search", "g then h/w/n/p   home, writeups, notes, projects", "t                next theme",
      "m                next interface", "e                effects on/off", "r                reader mode (articles)",
      "j / k           move through lists, enter opens", "?                this help", "esc              close"].forEach(function (l) { box.appendChild(el("div", null, l)); });
    help.appendChild(box); help.addEventListener("click", toggleHelp); document.body.appendChild(help);
  }
  function rows() { return $$("ul.list li,.grid2 article").filter(function (r) { return !r.hidden && r.offsetParent !== null; }); }
  function move(d) {
    var r = rows(); if (!r.length) return;
    if (cur >= 0 && r[cur]) r[cur].classList.remove("rowsel");
    cur = Math.max(0, Math.min(r.length - 1, cur + d));
    r[cur].classList.add("rowsel"); r[cur].scrollIntoView({ block: "center" });
  }
  addEventListener("keydown", function (e) {
    if (e.key === "Escape") {
      if (help) toggleHelp(); else if (root.dataset.reader === "on") setReader(false);
      return;
    }
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (/input|textarea|select/i.test((document.activeElement || {}).tagName || "")) return;
    var pal = $("#pal"); if (pal && !pal.hidden) return;
    var k = e.key;
    if (pendingG) {
      pendingG = false;
      var go = { h: "/", w: "/writeups", n: "/notes", p: "/projects" };
      if (go[k]) { e.preventDefault(); location.href = go[k]; }
      return;
    }
    if (k === "g") { pendingG = true; setTimeout(function () { pendingG = false; }, 1000); }
    else if (k === "t") setTheme(cycle(THEMES, root.dataset.theme));
    else if (k === "m") setUi(cycle(UIS, root.dataset.ui));
    else if (k === "e") setFx(root.dataset.fx !== "on");
    else if (k === "r" && prose) setReader(root.dataset.reader !== "on");
    else if (k === "?") toggleHelp();
    else if (k === "j") move(1);
    else if (k === "k") move(-1);
    else if (k === "Enter" && cur >= 0) { var r = rows()[cur], a = r && r.querySelector("a"); if (a) a.click(); }
  });

  // init sequence 
  thsel.addEventListener("change", function () { setTheme(thsel.value); });
  ifsel.addEventListener("change", function () { setUi(ifsel.value); });
  fxb.addEventListener("click", function () { setFx(root.dataset.fx !== "on"); });
  setTheme(root.dataset.theme || "amber");
  setFx(root.dataset.fx !== "off");
  setUi(root.dataset.ui || "classic");
})();
