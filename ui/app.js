"use strict";

const T = window.__TAURI__;
const invoke = T.core.invoke;
const src = (p) => T.core.convertFileSrc(p);
const $ = (s, el = document) => el.querySelector(s);
const $$ = (s, el = document) => [...el.querySelectorAll(s)];

// Opciones exactas de los metacampos de la tienda.
const KEYS = ["C", "C# / Db", "D", "D# / Eb", "E", "F", "F# / Gb", "G", "G# / Ab", "A", "A# / Bb", "B",
  "Cm", "C#m / Dbm", "Dm", "D#m / Ebm", "Em", "Fm", "F#m / Gbm", "Gm", "Am", "A#m / Bbm", "Bm"];
const GENEROS = ["R&B", "Hip-Hop", "Y2K", "Reggaetón", "Punk", "Newjazz", "Bossa nova", "POP", "Trap", "Plugg", "Afro", "Indie rock", "Soul", "Funk Brasileño"];
const MOODS = ["Groovy", "Sad", "Nostalgic", "Happy", "Dark", "Romantic", "Epic", "Sexy", "Hot"];
const INSTR = ["Guitar", "Piano", "Rhodes", "Pads", "Synth", "Flute", "Trumpet"];

// Los 7 pastel de las pastillas de la página de producto (fondo, texto).
const PASTEL = [["#fef3c7", "#92400e"], ["#fee2e2", "#991b1b"], ["#f3e8ff", "#6b21a8"], ["#e0e7ff", "#3730a3"],
  ["#dbeafe", "#1e40af"], ["#ccfbf1", "#115e59"], ["#dcfce7", "#166534"]];
// En la tienda: género en ámbar, mood en índigo, instrumento en menta.
const STORE_PILL = { genero: PASTEL[0], mood: PASTEL[3], instr: PASTEL[5] };

const STEP_TITLES = { archivos: "Zips", bucket: "Bucket", web: "Portada y miniatura", tienda: "Tienda", video: "Video", youtube: "YouTube" };
const STEP_ORDER = ["archivos", "bucket", "web", "tienda", "video", "youtube"];
const VIDEO_EXT = ["mp4", "mov", "webm", "mkv", "m4v"];
const VISUAL_EXT = ["jpg", "jpeg", "png", "webp", "gif", ...VIDEO_EXT];
const IMAGE_EXT = ["jpg", "jpeg", "png", "webp"];

const state = {
  slots: { wav: "", stems: "", mp3: "", visual: "", thumbnail: "" },
  chips: { genero: [], mood: [], instr: [] },
  colors: {},          // color pastel sorteado por pastilla
  detected: null,      // resultado de detect_wav
  thumbInfo: null,     // tamaño y peso de la miniatura
  duration: 0,
  settings: null,
  ytConnected: false,
  version: "",
  privacy: "public",   // visibilidad elegida para este beat
  job: null,
  view: "nuevo",
};

/* ---------- utilidades ---------- */

let toastTimer;
function toast(msg) {
  const t = $("#toast");
  t.textContent = msg;
  t.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove("show"), 2400);
}
const baseName = (p) => p.split(/[\\/]/).pop();
const ext = (p) => (p.split(".").pop() || "").toLowerCase();
const debounce = (fn, ms) => { let t; return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); }; };
const store = {
  get(k) { try { return JSON.parse(localStorage.getItem(k)); } catch { return null; } },
  set(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); } catch { /* sin almacenamiento */ } },
  del(k) { try { localStorage.removeItem(k); } catch { /* nada */ } },
};
const fmtTime = (s) => { s = Math.round(s || 0); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`; };
const fmtMB = (b) => `${(b / 1048576).toFixed(1).replace(".", ",")} MB`;
const keyShort = (k) => (k || "").split("/")[0].trim();
const el = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; };

/* ---------- vistas ---------- */

function show(view) {
  state.view = view;
  for (const v of ["nuevo", "trabajo", "ajustes", "mapa"]) $(`#view-${v}`).hidden = v !== view;
  $("#settingsBtn").classList.toggle("on", view === "ajustes");
  $("#mapBtn").classList.toggle("on", view === "mapa");
  if (view === "mapa") loadMap();
  if (view !== "trabajo") $$(".recent button").forEach((b) => b.classList.remove("on"));
  if (view === "ajustes") loadSettingsForm();
  if (view !== "nuevo") stopPreview();
}
$("#newBtn").addEventListener("click", () => { show("nuevo"); $("#f-name").focus(); });
const openSettings = () => show("ajustes");
$("#settingsBtn").addEventListener("click", openSettings);

/* ---------- recientes ---------- */

async function renderRecent() {
  const jobs = await invoke("list_jobs");
  const ul = $("#recent");
  ul.innerHTML = "";
  // Sin historial: solo aparecen los que quedaron sin terminar.
  const pending = jobs.filter((j) => !STEP_ORDER.every((s) => j.steps?.[s]?.status === "listo"));
  $("#recentLabel").hidden = !pending.length;
  for (const j of pending) {
    const done = false;
    const li = el("li");
    const b = el("button");
    b.type = "button";
    b.dataset.handle = j.handle;
    b.append(el("span", "n", j.input.name));
    if (done) {
      const ok = el("span", "st");
      ok.innerHTML = '<svg viewBox="0 0 16 16"><path d="M3.5 8.5l3 3 6-7"/></svg>';
      b.append(ok);
    } else b.append(el("span", "st", j.running ? "subiendo" : "a medias"));
    if (state.view === "trabajo" && state.job?.handle === j.handle) b.classList.add("on");
    b.addEventListener("click", () => openJob(j));
    li.append(b);
    ul.append(li);
  }
}

/* ---------- formulario ---------- */

function buildChips(id, list, group, max) {
  const box = $(id);
  for (const v of list) {
    const b = el("button", "chip", v);
    b.type = "button";
    b.dataset.v = v;
    b.addEventListener("click", () => {
      const arr = state.chips[group];
      const i = arr.indexOf(v);
      if (i >= 0) { arr.splice(i, 1); delete state.colors[`${group}:${v}`]; }
      else if (!max || arr.length < max) {
        arr.push(v);
        state.colors[`${group}:${v}`] = Math.floor(Math.random() * PASTEL.length);
      }
      paintChips();
      changed();
    });
    box.append(b);
  }
}

function paintChips() {
  for (const [group, id, max] of [["genero", "#c-genero"], ["mood", "#c-mood"], ["instr", "#c-instr", 3]]) {
    const sel = state.chips[group];
    $$(".chip", $(id)).forEach((b) => {
      const on = sel.includes(b.dataset.v);
      b.classList.toggle("on", on);
      b.setAttribute("aria-pressed", String(on));
      b.disabled = !!max && !on && sel.length >= max;
      if (on) {
        const k = `${group}:${b.dataset.v}`;
        if (state.colors[k] == null) state.colors[k] = Math.floor(Math.random() * PASTEL.length);
        const [bg, fg] = PASTEL[state.colors[k]];
        Object.assign(b.style, { background: bg, color: fg, borderColor: bg });
      } else Object.assign(b.style, { background: "", color: "", borderColor: "" });
    });
  }
}

function readInput() {
  return {
    name: $("#f-name").value.trim(),
    bpm: parseInt($("#f-bpm").value, 10) || 0,
    key: $("#f-key").value,
    artist: $("#f-artist").value.trim(),
    generos: [...state.chips.genero],
    moods: [...state.chips.mood],
    instruments: [...state.chips.instr],
    ...state.slots,
    ...publishFields(),
  };
}

/* ---------- publicación en YouTube ---------- */

function pad(n) { return String(n).padStart(2, "0"); }

function defaultWhen() {
  const d = new Date(Date.now() + 86400000);
  return { date: `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`, time: "18:00" };
}

function publishFields() {
  const privacy = state.privacy;
  if (privacy !== "scheduled") return { yt_privacy: privacy, yt_publish_at: "", yt_publish_ts: 0 };
  const date = $("#f-date").value, time = $("#f-time").value;
  const d = date && time ? new Date(`${date}T${time}:00`) : null;
  if (!d || isNaN(d)) return { yt_privacy: privacy, yt_publish_at: "", yt_publish_ts: 0, yt_date: date, yt_time: time };
  return { yt_privacy: privacy, yt_publish_at: d.toISOString().replace(".000Z", "Z"), yt_publish_ts: Math.floor(d.getTime() / 1000), yt_date: date, yt_time: time };
}

function setPrivacy(p) {
  state.privacy = ["public", "unlisted"].includes(p) ? p : "public";
  $$("#ytSeg button").forEach((b) => b.setAttribute("aria-checked", String(b.dataset.p === state.privacy)));
  $("#ytWhen").hidden = state.privacy !== "scheduled";
}

function renderPublish() {
  const box = $("#ytPub");
  box.classList.toggle("off", !state.ytConnected);
  const hint = $("#ytPubHint");
  hint.innerHTML = "";
  if (!state.ytConnected) {
    hint.append("Sin conectar: el video queda para subir a mano. ");
    const b = el("button", null, "Conectar en Ajustes");
    b.type = "button";
    b.addEventListener("click", openSettings);
    hint.append(b);
  } else if (!state.settings?.yt_audited) {
    hint.textContent = "Hasta que Google apruebe la auditoría, YouTube lo deja en privado";
  }
}

function writeInput(d) {
  $("#f-name").value = d.name || "";
  $("#f-bpm").value = d.bpm || "";
  $("#f-key").value = d.key || "";
  $("#f-artist").value = d.artist || "";
  state.chips = { genero: d.generos || [], mood: d.moods || [], instr: d.instruments || [] };
  const w = defaultWhen();
  $("#f-date").value = d.yt_date || w.date;
  $("#f-time").value = d.yt_time || w.time;
  setPrivacy(d.yt_privacy || state.settings?.yt_privacy || "public");
  state.colors = d.colors || {};
  state.detected = null;
  for (const k of Object.keys(state.slots)) setSlot(k, d[k] || "", true);
  paintChips();
  renderDetect();
}

function changed() {
  $("#formMsg").textContent = "";
  $$(".invalid").forEach((e) => e.classList.remove("invalid"));
  $$(".file.need").forEach((e) => e.classList.remove("need"));
  store.set("borrador", { ...readInput(), colors: state.colors });
  renderDetect();
  refreshPreview();
  refreshSuggestions();
}

function clearForm() {
  store.del("borrador");
  stopPreview();
  writeInput({});
  changed();
  $("#f-name").focus();
}

/* ---------- detección de BPM y tonalidad ---------- */

async function detectFromWav(path) {
  $("#bpmDet").textContent = "analizando…";
  $("#keyDet").textContent = "";
  let d;
  try { d = await invoke("detect_wav", { path }); } catch { d = null; }
  if (!d || state.slots.wav !== path) return;
  state.detected = d;
  const n = d.from_name || {};
  const a = d.audio;
  if (!$("#f-name").value.trim() && n.name) $("#f-name").value = n.name;
  if (!$("#f-bpm").value) $("#f-bpm").value = n.bpm || a?.bpm || "";
  if (!$("#f-key").value) $("#f-key").value = n.key || a?.key || "";
  changed();
}

function renderDetect() {
  const d = state.detected;
  const bpmDet = $("#bpmDet"), keyDet = $("#keyDet");
  bpmDet.innerHTML = ""; keyDet.innerHTML = "";
  const sub = $("#headSub");
  if (!d) {
    sub.textContent = state.slots.wav ? "Nombre, BPM y tonalidad: completalos a mano" : "Soltá los archivos en cualquier parte de la ventana";
    return;
  }
  const n = d.from_name || {}, a = d.audio;
  const parts = [n.name && "nombre", n.bpm && "BPM", n.key && "tonalidad"].filter(Boolean);
  const said = parts.join(", ").replace(/, ([^,]*)$/, " y $1");
  sub.textContent = parts.length ? `${said.charAt(0).toUpperCase()}${said.slice(1)} ${parts.length > 1 ? "leídos" : "leído"} del archivo` : "BPM y tonalidad detectados del audio";

  const bpm = parseInt($("#f-bpm").value, 10) || 0;
  if (bpm) {
    const fromName = n.bpm && n.bpm === bpm;
    const fromAudio = a && a.bpm === bpm;
    if (fromName || fromAudio) bpmDet.append(el("span", "src", fromName ? "del archivo" : "detectado"));
    for (const [label, f] of [["×2", 2], ["÷2", 0.5]]) {
      const b = el("button", null, label);
      b.type = "button";
      b.addEventListener("click", () => { $("#f-bpm").value = Math.round(bpm * f); changed(); });
      bpmDet.append(b);
    }
    if (a && n.bpm && a.bpm !== n.bpm && Math.abs(a.bpm - n.bpm) > 2) bpmDet.append(el("span", "warn", `el audio suena a ${a.bpm}`));
  }
  const key = $("#f-key").value;
  if (key) {
    const fromName = n.key && n.key === key;
    const fromAudio = a && (a.key === key);
    if (fromName || fromAudio) keyDet.append(el("span", "src", fromName ? "del archivo" : "detectado"));
    const alt = a ? (a.key === key ? a.alt_key : a.alt_key === key ? a.key : null) : null;
    if (alt) {
      const b = el("button", null, `o ${keyShort(alt)}`);
      b.type = "button";
      b.addEventListener("click", () => { $("#f-key").value = alt; changed(); });
      keyDet.append(b);
    }
    if (a && n.key && a.key !== n.key && a.alt_key !== n.key) keyDet.append(el("span", "warn", `el audio suena a ${keyShort(a.key)}`));
  }
}

/* ---------- archivos ---------- */

function setSlot(slot, path, silent) {
  state.slots[slot] = path;
  const row = $(`.file[data-slot="${slot}"]`);
  if (row) {
    row.classList.toggle("filled", !!path);
    const v = $(".fval", row);
    v.textContent = path ? baseName(path) : v.dataset.empty;
  }
  if (slot === "mp3") loadAudio(path);
  if (slot === "thumbnail") loadThumbInfo(path);
  if (slot === "wav" && path && !silent) detectFromWav(path);
  if (slot === "visual") { stopPreview(); loadCandidates(0); }
  if (!silent) changed();
}

async function loadAudio(path) {
  stopPreview();
  const audio = $("#audio");
  state.duration = 0;
  if (!path) { audio.removeAttribute("src"); return; }
  audio.src = src(path);
  const d = await invoke("media_duration", { path }).catch(() => null);
  if (state.slots.mp3 === path && d) { state.duration = d; refreshPreview(); }
}

async function loadThumbInfo(path) {
  state.thumbInfo = null;
  if (!path) return;
  const [info] = await invoke("path_info", { paths: [path] });
  if (state.slots.thumbnail === path) { state.thumbInfo = info; refreshPreview(); }
}

/** Ubica cada cosa arrastrada en su lugar, aunque no caiga justo encima. */
async function place(paths, target) {
  const infos = await invoke("path_info", { paths });
  const unplaced = [];
  for (const f of infos) {
    const fits = (s) => ({ wav: f.ext === "wav", mp3: f.ext === "mp3", stems: f.is_dir, visual: VISUAL_EXT.includes(f.ext) }[s]);
    let slot = null;
    if (target && fits(target)) slot = target;
    else if (f.is_dir) slot = "stems";
    else if (f.ext === "wav") slot = "wav";
    else if (f.ext === "mp3") slot = "mp3";
    else if (VISUAL_EXT.includes(f.ext)) slot = "visual";
    if (slot) {
      setSlot(slot, f.path);
      if (slot === "stems" && f.wav_count === 0) toast("Esa carpeta no tiene archivos .wav");
    } else unplaced.push(f.name);
  }
  if (unplaced.length) toast(`No sé dónde va ${unplaced.join(", ")}`);
}

const FILTERS = {
  wav: [{ name: "WAV", extensions: ["wav"] }],
  mp3: [{ name: "MP3", extensions: ["mp3"] }],
  visual: [{ name: "Foto o video", extensions: VISUAL_EXT }],
};
$$(".file").forEach((row) => row.addEventListener("click", async () => {
  const slot = row.dataset.slot;
  const picked = await T.dialog.open({ directory: slot === "stems", multiple: false, filters: FILTERS[slot] });
  if (picked) place([picked], slot);
}));

async function initDragDrop() {
  let over = null;
  await T.webview.getCurrentWebview().onDragDropEvent((ev) => {
    const p = ev.payload;
    const dpr = window.devicePixelRatio || 1;
    if (p.type === "enter" || p.type === "over") {
      document.body.classList.add("dragging");
      const hit = p.position ? document.elementFromPoint(p.position.x / dpr, p.position.y / dpr) : null;
      const z = hit ? hit.closest(".file") : null;
      if (z !== over) { over?.classList.remove("over"); z?.classList.add("over"); over = z; }
    } else {
      document.body.classList.remove("dragging");
      over?.classList.remove("over");
      if (p.type === "drop" && p.paths?.length) {
        if (state.view !== "nuevo") show("nuevo");
        place(p.paths, over?.dataset.slot);
      }
      over = null;
    }
  });
}

/* ---------- vista previa ---------- */

let pvView = "yt";
function setPreview(v) {
  pvView = v;
  $(".switch").classList.toggle("st", v === "st");
  $$(".opt").forEach((o) => {
    const on = o.dataset.v === v;
    o.classList.toggle("act", on);
    o.setAttribute("aria-selected", String(on));
    o.tabIndex = on ? 0 : -1;
  });
  $("#pv-yt").hidden = v !== "yt";
  $("#pv-st").hidden = v !== "st";
  if (v !== "yt") stopPreview();
}
$$(".opt").forEach((o) => {
  o.addEventListener("click", () => setPreview(o.dataset.v));
  o.addEventListener("keydown", (e) => {
    if (["ArrowLeft", "ArrowRight", "Enter", " "].includes(e.key)) {
      e.preventDefault();
      const next = e.key === "Enter" || e.key === " " ? o.dataset.v : (pvView === "yt" ? "st" : "yt");
      setPreview(next);
      $(`.opt[data-v="${next}"]`).focus();
    }
  });
});

/** Visual en un contenedor: video o gif en loop, o imagen. */
function mountVisual(box, path, { muted = true, autoplay = false } = {}) {
  const prev = box.dataset.path || "";
  if (prev === path) return box.querySelector("video");
  box.dataset.path = path;
  box.innerHTML = "";
  if (!path) return null;
  if (VIDEO_EXT.includes(ext(path))) {
    const v = document.createElement("video");
    Object.assign(v, { src: src(path), loop: true, muted, playsInline: true, autoplay, preload: "auto" });
    // Sin reproducir, se muestra el primer cuadro en lugar de un recuadro vacío.
    v.addEventListener("loadeddata", () => { if (v.paused && v.currentTime === 0) v.currentTime = 0.05; }, { once: true });
    box.append(v);
    return v;
  }
  const img = document.createElement("img");
  img.src = src(path);
  img.alt = "";
  box.append(img);
  return null;
}

function stopPreview() {
  const a = $("#audio");
  if (!a.paused) a.pause();
  $("#player").classList.remove("playing");
  const v = $("#playerVisual video");
  if (v) v.pause();
}

$("#playBtn").addEventListener("click", async () => {
  const a = $("#audio");
  const player = $("#player");
  const v = $("#playerVisual video");
  if (!state.slots.mp3) { toast("Soltá el mp3 con tag para escuchar la vista previa"); return; }
  if (player.classList.contains("playing")) { stopPreview(); return; }
  try { await a.play(); } catch { toast("No se pudo reproducir el mp3"); return; }
  if (v) v.play().catch(() => {});
  player.classList.add("playing");
});
$("#audio").addEventListener("timeupdate", (e) => {
  const a = e.target;
  $("#playBar").style.width = a.duration ? `${(a.currentTime / a.duration) * 100}%` : "0";
});
$("#audio").addEventListener("ended", stopPreview);

function metaRows(dl, rows) {
  dl.innerHTML = "";
  for (const [dt, dd, bad] of rows) {
    const div = el("div", bad ? "bad" : null);
    div.append(el("dt", null, dt), el("dd", null, dd));
    dl.append(div);
  }
}

const refreshPreview = debounce(async () => {
  const input = readInput();
  let p = { title: "", description: "", tags: "", handle: "" };
  if (input.name) p = await invoke("preview", { input }).catch(() => p);
  const title = p.title || "Título del video";

  // YouTube
  $("#ytTitleS").textContent = title;
  $("#ytTitleV").textContent = title;
  $("#ytDur").textContent = fmtTime(state.duration);
  $("#ytThumb").style.backgroundImage = state.slots.thumbnail ? `url("${src(state.slots.thumbnail)}")` : "";
  $("#saveThumbPick").hidden = !state.slots.thumbnail;
  mountVisual($("#playerVisual"), state.slots.visual);
  const lines = (p.description || "").split("\n").slice(0, 3).join("\n");
  const desc = $("#ytDesc");
  desc.textContent = lines || "La descripción aparece cuando completás el nombre.";
  if (lines) desc.append(el("span", "more", "\n…más"));

  const tlen = [...(p.title || "")].length;
  const tagLen = (p.tags || "").length;
  const ti = state.thumbInfo;
  const thumbOk = ti && ti.width === 1280 && ti.height === 720 && ti.size < 2 * 1048576;
  metaRows($("#ytMeta"), [
    ["Título", tlen ? `${tlen} / 100 · ${tlen <= 70 ? "se lee completo" : "se corta en búsqueda"}` : "—", tlen > 100],
    ["Tags", tagLen ? `${tagLen} / 500` : "—", tagLen > 500],
    ["Miniatura", ti ? `${ti.width} × ${ti.height} · ${fmtMB(ti.size)}${thumbOk ? "" : " · se adapta al publicar"}` : "—"],
    ["Duración", state.duration ? fmtTime(state.duration) : "—"],
  ]);

  // Tienda
  $("#stTitle").textContent = input.name || "Nombre del beat";
  mountVisual($("#stImg"), state.slots.visual, { autoplay: true });
  const specs = $("#stSpecs");
  specs.innerHTML = "";
  const spec = [input.bpm ? `${input.bpm} BPM` : null, input.key ? keyShort(input.key) : null].filter(Boolean).join(" · ");
  if (spec) specs.append(el("span", null, spec));
  const firsts = [["genero", input.generos[0]], ["mood", input.moods[0]], ["instr", input.instruments[0]]].filter(([, v]) => v);
  if (spec && firsts.length) specs.append(el("span", "dv", "•"));
  for (const [g, v] of firsts) {
    const pill = el("span", "pill", v);
    pill.style.background = STORE_PILL[g][0];
    pill.style.color = STORE_PILL[g][1];
    specs.append(pill);
  }
  const money = (v) => { const n = parseFloat(String(v).replace(",", ".")); return Number.isFinite(n) ? (Number.isInteger(n) ? n : n.toFixed(2)) : "—"; };
  $("#stVarP").textContent = `WAV Premium — $${money(state.settings?.price_premium)} USD`;
  $("#stVarE").textContent = `WAV + STEMS Exclusiva — $${money(state.settings?.price_exclusiva)} USD`;
  const vis = state.slots.visual;
  metaRows($("#stMeta"), [
    ["Portada", !vis ? "—" : VIDEO_EXT.includes(ext(vis)) ? `gif ${state.settings?.gif_size || 600} × ${state.settings?.gif_size || 600} · se crea al publicar` : ext(vis) === "gif" ? "gif tal cual" : "imagen tal cual"],
    ["Catálogo", vis ? "miniatura WebP de 240 px · se crea al publicar" : "—"],
    ["Link", p.handle ? `/beats/${p.handle}` : "—"],
  ]);
}, 150);

/* ---------- miniatura: fotogramas para elegir ---------- */

let thumbSeed = 0;
async function loadCandidates(seed) {
  thumbSeed = seed;
  const vis = state.slots.visual;
  const box = $("#thumbPick"), grid = $("#thumbGrid");
  if (!vis) { box.hidden = true; setThumb(""); return; }
  grid.classList.add("loading");
  let list = [];
  try { list = await invoke("thumb_candidates", { visual: vis, seed }); } catch (e) { toast(String(e)); }
  if (state.slots.visual !== vis || thumbSeed !== seed) return;
  grid.classList.remove("loading");
  grid.innerHTML = "";
  // Con una foto hay una sola opción: no hace falta elegir.
  box.hidden = list.length < 2;
  list.forEach((p, i) => {
    const b = el("button");
    b.type = "button";
    b.setAttribute("role", "radio");
    b.setAttribute("aria-label", `Fotograma ${i + 1}`);
    const img = el("img");
    img.src = `${src(p)}?v=${Date.now()}`;
    img.alt = "";
    b.append(img);
    b.dataset.p = p;
    b.addEventListener("click", () => setThumb(p));
    grid.append(b);
  });
  setThumb(list[Math.floor((list.length - 1) / 2)] || "");
}

function setThumb(p) {
  state.slots.thumbnail = p;
  $$("#thumbGrid button").forEach((b) => b.setAttribute("aria-checked", String(b.dataset.p === p)));
  loadThumbInfo(p);
  refreshPreview();
}
$("#moreThumbs").addEventListener("click", () => loadCandidates(thumbSeed + 1));
$("#saveThumbPick").addEventListener("click", () => guardarMiniatura(state.slots.thumbnail, $("#f-name").value));

/* ---------- publicar ---------- */

function markInvalid(msg) {
  const map = [["nombre", "#f-name"], ["BPM", "#f-bpm"], ["tonalidad", "#f-key"], ["artista", "#f-artist"]];
  for (const [w, sel] of map) if (msg.includes(w)) $(sel).classList.add("invalid");
  for (const [w, slot] of [["WAV", "wav"], ["stems", "stems"], ["mp3", "mp3"], ["multimedia", "visual"]]) {
    if (msg.includes(w)) $(`.file[data-slot="${slot}"]`).classList.add("need");
  }
}

async function publish() {
  const input = readInput();
  const btn = $("#publishBtn");
  try { await invoke("validate_input", { input }); } catch (e) {
    $("#formMsg").textContent = e;
    markInvalid(String(e));
    return;
  }
  btn.disabled = true;
  btn.textContent = "Revisando…";
  let pre;
  try {
    pre = await invoke("preflight", { input });
  } catch (e) {
    $("#formMsg").textContent = e;
    btn.disabled = false;
    btn.textContent = "Publicar beat";
    return;
  }
  btn.textContent = "Publicar beat";
  if (pre.exists) {
    const choice = await askDuplicate(input.name);
    if (choice !== "replace") {
      btn.disabled = false;
      if (choice === "rename") { $("#f-name").focus(); $("#f-name").select(); }
      return;
    }
    input.replace = true;
  }
  try {
    const handle = await invoke("start_job", { input });
    store.del("borrador");
    stopPreview();
    openJob({ handle, input, steps: {}, running: true });
    renderRecent();
  } catch (e) {
    $("#formMsg").textContent = e;
  } finally {
    btn.disabled = false;
  }
}
$("#publishBtn").addEventListener("click", publish);

function askDuplicate(name) {
  return new Promise((resolve) => {
    const m = $("#dupModal");
    $("#dupTitle").textContent = `Ya hay un beat que se llama "${name}"`;
    m.hidden = false;
    $("#dupReplace").focus();
    const done = (v) => { m.hidden = true; document.removeEventListener("keydown", onKey); resolve(v); };
    const onKey = (e) => { if (e.key === "Escape") done("cancel"); };
    document.addEventListener("keydown", onKey);
    $("#dupReplace").onclick = () => done("replace");
    $("#dupRename").onclick = () => done("rename");
    m.onclick = (e) => { if (e.target === m) done("cancel"); };
  });
}
$("#clearBtn").addEventListener("click", clearForm);

/* ---------- trabajo ---------- */

function openJob(job) {
  state.job = job;
  $("#steps").innerHTML = "";
  show("trabajo");
  renderJob();
  $$(".recent button").forEach((b) => b.classList.toggle("on", b.dataset.handle === job.handle));
}

function renderJob() {
  const j = state.job;
  if (!j) return;
  $("#jobName").textContent = j.input.name;
  const done = STEP_ORDER.every((s) => j.steps?.[s]?.status === "listo");
  $("#jobSub").textContent = j.running ? "Subiendo. Podés seguir con otra cosa mientras tanto."
    : done ? (j.yt_auto ? "Publicado en la tienda y en YouTube." : "Publicado. Falta solo subir el video en YouTube Studio.")
      : j.error ? "Se detuvo antes de terminar." : "";

  const ol = $("#steps");
  if (!ol.children.length) {
    for (const s of STEP_ORDER) {
      const li = el("li", "step");
      li.dataset.step = s;
      li.innerHTML = `<span class="sdot"></span><span class="step-title">${STEP_TITLES[s]}</span><span class="step-detail"><span class="txt"></span><span class="meter" hidden><i></i></span></span>`;
      ol.append(li);
    }
  }
  for (const s of STEP_ORDER) {
    const st = j.steps?.[s] || { status: "pendiente", detail: "" };
    const li = $(`.step[data-step="${s}"]`, ol);
    li.dataset.s = st.status;
    $(".txt", li).textContent = st.detail || (st.status === "pendiente" ? "En espera" : "");
    const m = $(".meter", li);
    m.hidden = st.status !== "en_curso";
    if (st.status !== "en_curso") $("i", m).style.width = "0";
  }
  $("#jobError").hidden = j.running || !j.error;
  $("#jobErrorText").textContent = j.error || "";
  $("#result").hidden = !done;
  if (done) {
    $("#jobTitle").textContent = j.yt_title;
    $("#jobDesc").textContent = j.yt_description;
    $("#jobTags").textContent = j.yt_tags;
    const img = $("#jobThumb");
    img.hidden = !j.thumbnail;
    if (j.thumbnail) img.src = `${src(j.thumbnail)}?v=${j.created}`;
    $("#saveThumb").hidden = !j.thumbnail;
    $("#copyYtLink").hidden = !j.yt_auto;
    $("#uploadYt").hidden = j.yt_auto || !state.ytConnected;
    $("#revealVideo").hidden = !!j.yt_auto;
    $("#openStudio").textContent = j.yt_auto ? "Abrir en YouTube Studio" : "Abrir YouTube Studio";
    $("#ytNote").hidden = !j.yt_note;
    $("#ytNote").textContent = j.yt_note || "";
    $("#thumbNote").hidden = !j.thumbnail_note;
    $("#thumbNote").textContent = j.thumbnail_note ? `Miniatura: ${j.thumbnail_note}` : "";
  }
}

function onProgress({ step, detail, progress }) {
  const li = $(`.step[data-step="${step}"]`);
  if (!li || state.job?.steps?.[step]?.status !== "en_curso") return;
  $(".txt", li).textContent = `${detail} ${Math.round(progress * 100)}%`;
  const m = $(".meter", li);
  m.hidden = false;
  $("i", m).style.width = `${(progress * 100).toFixed(1)}%`;
}

$("#discardBtn").addEventListener("click", async () => {
  try { await invoke("forget_job", { handle: state.job.handle }); } catch {}
  state.job = null;
  show("nuevo");
  renderRecent();
  toast("Descartado. Lo que ya se había subido queda como está.");
});
$("#retryBtn").addEventListener("click", async () => {
  try {
    await invoke("retry_job", { handle: state.job.handle });
    state.job.running = true;
    state.job.error = "";
    renderJob();
  } catch (e) { toast(String(e)); }
});
$$("[data-open]").forEach((b) => b.addEventListener("click", () => {
  const url = state.job?.[b.dataset.open];
  if (url) T.opener.openUrl(url);
}));
$$("[data-copy]").forEach((b) => b.addEventListener("click", async () => {
  await T.clipboardManager.writeText(state.job?.[b.dataset.copy] || "");
  b.textContent = "Copiado";
  b.classList.add("done");
  setTimeout(() => { b.textContent = "Copiar"; b.classList.remove("done"); }, 1400);
}));
 // Copia de la miniatura de YouTube (1280×720, con las franjas negras), donde elijas
$("#saveThumb").addEventListener("click", () => guardarMiniatura(state.job?.thumbnail, state.job?.input?.name));

async function guardarMiniatura(origen, beat) {
  if (!origen) return toast("Elegí una miniatura primero");
  const j = { thumbnail: origen, input: { name: beat } };
  const e = ext(j.thumbnail) || "jpg";
  const nombre = `${(j.input?.name || "beat").trim()} - miniatura.${e}`.replace(/[\\/:*?"<>|]/g, "");
  const destino = await T.dialog.save({ defaultPath: nombre, filters: [{ name: "Imagen", extensions: [e] }] });
  if (!destino) return;
  try {
    await invoke("guardar_miniatura", { origen: j.thumbnail, destino });
    toast("Miniatura guardada");
  } catch (err) { toast(String(err)); }
}

$("#revealVideo").addEventListener("click", () => { if (state.job?.video) T.opener.revealItemInDir(state.job.video); });
$("#copyYtLink").addEventListener("click", async () => {
  if (!state.job?.yt_url) return;
  await T.clipboardManager.writeText(state.job.yt_url);
  toast("Link copiado");
});
$("#uploadYt").addEventListener("click", async () => {
  try {
    await invoke("upload_youtube", { handle: state.job.handle });
    state.job.running = true;
    renderJob();
  } catch (e) { toast(String(e)); }
});
$("#openStudio").addEventListener("click", async () => {
  if (state.job?.yt_auto && state.job.yt_video_id) {
    T.opener.openUrl(`https://studio.youtube.com/video/${state.job.yt_video_id}/edit`);
    return;
  }
  if (state.job?.yt_title) await T.clipboardManager.writeText(state.job.yt_title);
  if (state.job?.video) T.opener.revealItemInDir(state.job.video);
  T.opener.openUrl("https://www.youtube.com/upload");
  toast("Título copiado. Arrastrá el video a Studio.");
});

/* ---------- ajustes ---------- */

async function loadSettings() {
  const v = await invoke("get_settings");
  state.settings = v.settings;
  state.ytConnected = v.yt_connected;
  state.version = v.version;
  state.view = v;
  return v;
}

async function loadSettingsForm() {
  const v = await loadSettings();
  const f = $("#settingsForm");
  for (const [k, val] of Object.entries(v.settings)) {
    const e = f.elements[k];
    if (!e) continue;
    if (e.type === "checkbox") e.checked = !!val; else e.value = val;
  }
  $('[data-secret="tienda"]').placeholder = v.tienda_de_beatstore ? "Se usa la de la app beatstore" : v.has_tienda ? "Guardada (escribí para cambiarla)" : "Sin cargar";
  $('[data-secret="r2"]').placeholder = v.has_r2_secret ? "Guardado (escribí para cambiarlo)" : "Sin cargar";
  $('[data-secret="yt"]').placeholder = v.has_yt_secret ? "Guardado (escribí para cambiarlo)" : "Sin cargar";
  $('[data-secret="notion"]').placeholder = v.has_notion ? "Guardado (escribí para cambiarlo)" : "Sin cargar";
  $('[data-secret="ytkey"]').placeholder = v.has_yt_key ? "Guardado (escribí para cambiarlo)" : "Sin cargar";
  renderConn();
  $("#verText").textContent = `Subir Beat ${v.version}`;
  $$("[data-secret]").forEach((e) => (e.value = ""));
}

$("#settingsForm").addEventListener("submit", (e) => { e.preventDefault(); saveSettingsForm(); });

async function saveSettingsForm(quiet) {
  const f = $("#settingsForm");
  const s = { ...state.settings };
  for (const k of Object.keys(s)) {
    const field = f.elements[k];
    if (!field) continue;
    if (field.type === "checkbox") s[k] = field.checked;
    else if (typeof s[k] === "number") s[k] = parseInt(field.value, 10) || s[k];
    else s[k] = field.value.trim();
  }
  try {
    await invoke("save_settings", { settings: s, tiendaSecret: $('[data-secret="tienda"]').value, r2Secret: $('[data-secret="r2"]').value, ytSecret: $('[data-secret="yt"]').value, notionSecret: $('[data-secret="notion"]').value, ytKey: $('[data-secret="ytkey"]').value });
    await loadSettingsForm();
    refreshPreview();
    renderPublish();
    if (!quiet) toast("Ajustes guardados");
    return true;
  } catch (err) { toast(String(err)); return false; }
}

$("#testBtn").addEventListener("click", async () => {
  const ul = $("#checks");
  ul.innerHTML = "<li>Probando…</li>";
  const r = await invoke("test_connections");
  ul.innerHTML = "";
  for (const [name, c] of [["Tienda", r.tienda], ["R2", r.r2], ["ffmpeg", r.ffmpeg]]) {
    const li = el("li", c.ok ? "ok" : null, `${name}: ${c.detail}`);
    li.title = c.detail;
    ul.append(li);
  }
});
$("#pickOut").addEventListener("click", async () => {
  const p = await T.dialog.open({ directory: true });
  if (p) $("#s-out").value = p;
});

function renderConn() {
  const on = state.ytConnected;
  $("#ytConn .cdot").classList.toggle("on", on);
  const t = $("#ytConnText");
  t.textContent = "";
  if (on && state.settings?.yt_channel) {
    t.append("Conectado como ");
    t.append(el("b", null, state.settings.yt_channel));
  } else t.textContent = on ? "Conectado" : "Sin conectar";
  $("#ytConnBtn").textContent = on ? "Desconectar" : "Conectar con YouTube";
}

$("#ytConnBtn").addEventListener("click", async () => {
  const btn = $("#ytConnBtn");
  if (state.ytConnected) {
    try { await invoke("yt_disconnect"); } catch (e) { toast(String(e)); }
    await loadSettingsForm();
    renderPublish();
    return;
  }
  // Guarda primero lo que haya escrito (ID y secreto) para usarlo al conectar.
  if (!(await saveSettingsForm(true))) return;
  btn.disabled = true;
  btn.textContent = "Esperando el navegador…";
  try {
    await invoke("yt_connect");
    toast("YouTube conectado");
  } catch (e) { toast(String(e)); }
  btn.disabled = false;
  await loadSettingsForm();
  renderPublish();
});

/* ---------- actualizaciones ---------- */

async function checkUpdate(manual) {
  const btn = $("#updBtn");
  if (manual) { btn.disabled = true; btn.textContent = "Buscando…"; }
  try {
    const u = await invoke("check_update");
    if (u) {
      $("#updText").innerHTML = "";
      $("#updText").append("Hay una versión nueva: ", el("b", null, u.version));
      if (u.notes) $("#upd").title = u.notes;
      $("#upd").hidden = false;
      if (manual) $("#verText").textContent = `Subir Beat ${state.version} · Hay una versión nueva: ${u.version}`;
    } else if (manual) {
      $("#verText").textContent = `Subir Beat ${state.version} · Al día`;
    }
  } catch (e) {
    if (manual) toast(String(e));
  }
  if (manual) { btn.disabled = false; btn.textContent = "Buscar actualizaciones"; }
}

$("#updBtn").addEventListener("click", () => checkUpdate(true));
$("#updLater").addEventListener("click", () => ($("#upd").hidden = true));
$("#updInstall").addEventListener("click", async () => {
  const b = $("#updInstall");
  b.disabled = true;
  b.textContent = "Descargando…";
  try {
    await invoke("install_update");
  } catch (e) {
    toast(String(e));
    b.disabled = false;
    b.textContent = "Instalar y reiniciar";
  }
});

/* ---------- mapa de artistas ---------- */

const GROUPS = [["rnb", "R&B y urbano"], ["rap", "Rap"], ["trap", "Trap"], ["otro", "Otros"]];
const COUNTRY = { MX: "México", US: "EE. UU.", otro: "Otro" };
const mapUI = { country: "", groups: new Set(), q: "", editing: null };
const DAY = 86400;

function barsEl(o) {
  const lv = o ? o.level : 0;
  const b = el("span", `bars b${lv}`);
  b.append(el("i"), el("i"), el("i"));
  b.title = o ? `${o.label}. ${o.why}` : "Sin medir";
  return b;
}

async function loadMap() {
  try { state.map = await invoke("artists_get"); } catch (e) { toast(String(e)); return; }
  renderMap();
}

$("#mapBtn").addEventListener("click", () => show("mapa"));
$("#sugMap").addEventListener("click", () => show("mapa"));

function renderMap() {
  const m = state.map;
  if (!m) return;
  const synced = m.synced ? new Date(m.synced * 1000).toLocaleDateString("es-AR", { day: "numeric", month: "long" }) : null;
  $("#mapSub").textContent = `${m.artists.length} artistas` + (synced ? ` · ${m.total_requests} pedidos leídos el ${synced}` : " · todavía sin leer los pedidos de Notion");

  // Lo que te piden
  const asks = [...m.artists].filter((a) => a.requests > 0).sort((a, b) => b.requests - a.requests).slice(0, 4);
  const box = $("#mapAsks");
  box.innerHTML = "";
  box.hidden = !asks.length;
  if (asks.length) {
    box.append(el("p", "lbl", "Lo que más te piden"));
    const grid = el("div", "asks-grid");
    for (const a of asks) {
      const c = el("button", "ask");
      c.type = "button";
      const where = [a.requests_mx && `${a.requests_mx} de México`, a.requests_us && `${a.requests_us} de EE. UU.`].filter(Boolean).join(", ");
      c.append(el("b", null, a.name), el("span", "req", `${a.requests} ${a.requests === 1 ? "pedido" : "pedidos"}`), el("p", null, where || "Sin país identificado"));
      c.addEventListener("click", () => { mapUI.editing = a.id; renderMap(); });
      grid.append(c);
    }
    box.append(grid);
  }

  // Filtros
  $$("#mapCountry button").forEach((b) => b.setAttribute("aria-checked", String(b.dataset.c === mapUI.country)));
  const gbox = $("#mapGroups");
  if (!gbox.children.length) {
    for (const [g, label] of GROUPS) {
      const c = el("button", "chip", label);
      c.type = "button";
      c.dataset.g = g;
      c.addEventListener("click", () => { mapUI.groups.has(g) ? mapUI.groups.delete(g) : mapUI.groups.add(g); renderMap(); });
      gbox.append(c);
    }
  }
  $$("#mapGroups .chip").forEach((c) => c.classList.toggle("on", mapUI.groups.has(c.dataset.g)));

  // Tabla
  const q = mapUI.q.trim().toLowerCase();
  const list = m.artists
    .filter((a) => !mapUI.country || a.country === mapUI.country)
    .filter((a) => !mapUI.groups.size || mapUI.groups.has(a.group))
    .filter((a) => !q || [a.name, ...a.aliases, a.style, a.vibe, a.city].join(" ").toLowerCase().includes(q))
    .sort(rankArtists);
  const t = $("#mapTable");
  t.innerHTML = "";
  const head = el("div", "mrow mhead");
  for (const h of ["Artista", "Estilo", "Vibe e instrumentos", "BPM", "Nivel", "Oportunidad"]) head.append(el("span", null, h));
  t.append(head);
  if (mapUI.editing === "") t.append(editor(null));
  for (const a of list) t.append(mapUI.editing === a.id ? editor(a) : row(a));
  if (!list.length) t.append(el("p", "empty-row", "No hay artistas con esos filtros."));

  // Pedidos sin artista del mapa
  const un = $("#mapUnmatched");
  un.innerHTML = "";
  un.hidden = !m.unmatched.length;
  if (m.unmatched.length) {
    un.append(el("p", "lbl", `Pedidos que no nombran a nadie del mapa (${m.unmatched.length})`));
    for (const r of m.unmatched) {
      const li = el("div", "unm");
      li.append(el("span", "unm-c", `${COUNTRY[r.country] || "Otro"} · ${r.date}`), el("p", null, r.text));
      un.append(li);
    }
    un.append(el("p", "help", "Si alguno nombra a un artista que te sirve, sumalo con \"Agregar artista\" y la próxima lectura lo va a contar."));
  }

  // Descartados
  const d = $("#mapDiscarded");
  d.hidden = !m.discarded.length;
  $("summary", d).textContent = `Descartados (${m.discarded.length})`;
  const dl = $(".disc-list", d);
  dl.innerHTML = "";
  for (const a of m.discarded) {
    const r = el("div", "disc");
    r.append(el("span", null, a.name));
    const b = el("button", "link", "Recuperar");
    b.type = "button";
    b.addEventListener("click", async () => { state.map = await invoke("artist_restore", { id: a.id }); renderMap(); refreshSuggestions(); });
    r.append(b);
    dl.append(r);
  }
}

/** Orden: más pedidos, mejor oportunidad, R&B primero, menos famosos antes. */
function rankArtists(a, b) {
  const lv = (x) => (x.opp ? x.opp.level : 1.5);
  const big = (x) => (/grande/i.test(x.level) ? 1 : 0);
  const g = (x) => ({ rnb: 0, rap: 1, trap: 2 }[x.group] ?? 3);
  return (b.requests - a.requests) || (lv(b) - lv(a)) || (big(a) - big(b)) || (g(a) - g(b)) || a.name.localeCompare(b.name);
}

function row(a) {
  const r = el("button", "mrow");
  r.type = "button";
  const n = el("span", "n");
  n.append(el("b", null, a.name));
  if (a.requests) n.append(el("span", "req", String(a.requests)));
  if (a.new) n.append(el("span", "newb", "nuevo"));
  n.append(el("small", null, [COUNTRY[a.country] || "", a.city].filter(Boolean).join(" · ")));
  const bpm = a.bpm_min ? (a.bpm_max && a.bpm_max !== a.bpm_min ? `${a.bpm_min}–${a.bpm_max}` : String(a.bpm_min)) : "—";
  r.append(n, el("span", "muted", a.style || "—"), el("span", "muted", a.vibe || "—"), el("span", "muted", bpm), el("span", "muted", a.level || "—"), barsEl(a.opp));
  r.addEventListener("click", () => { mapUI.editing = a.id; renderMap(); });
  return r;
}

function editor(a) {
  const isNew = !a;
  a = a || { id: "", name: "", aliases: [], country: "MX", city: "", group: "rnb", style: "", vibe: "", bpm_min: 0, bpm_max: 0, level: "Emergente" };
  const f = el("form", "medit");
  const field = (label, input) => { const w = el("label", "mf"); w.append(el("span", null, label), input); return w; };
  const inp = (v, ph = "") => { const i = el("input"); i.value = v ?? ""; i.placeholder = ph; return i; };
  const sel = (v, opts) => { const s = el("select"); for (const [k, l] of opts) { const o = el("option", null, l); o.value = k; s.append(o); } s.value = v; return s; };
  const name = inp(a.name, "Nombre"), aliases = inp(a.aliases.join(", "), "Otros nombres, separados por coma");
  const country = sel(a.country, [["MX", "México"], ["US", "EE. UU."], ["otro", "Otro"]]), city = inp(a.city, "Ciudad");
  const group = sel(a.group, GROUPS), style = inp(a.style, "Rap melódico con R&B"), vibe = inp(a.vibe, "Romántico, guitarras");
  const bmin = inp(a.bpm_min || "", "80"), bmax = inp(a.bpm_max || "", "95"), level = sel(a.level, [["Emergente", "Emergente"], ["Intermedio", "Intermedio"], ["Grande", "Grande"], ["A confirmar", "A confirmar"]]);
  if (![...level.options].some((o) => o.value === a.level)) { const o = el("option", null, a.level); o.value = a.level; level.append(o); level.value = a.level; }
  const grid = el("div", "mgrid");
  grid.append(field("Nombre", name), field("También aparece como", aliases), field("País", country), field("Ciudad", city), field("Grupo", group), field("Estilo", style), field("Vibe e instrumentos", vibe), field("BPM desde", bmin), field("BPM hasta", bmax), field("Nivel", level));
  f.append(grid);
  if (a.opp) f.append(el("p", "help", `${a.opp.label}: ${a.opp.why} (medido el ${new Date(a.opp.measured * 1000).toLocaleDateString("es-AR")})`));
  const act = el("div", "medit-actions");
  const btn = (t, cls, fn) => { const b = el("button", `btn small ${cls}`, t); b.type = "button"; b.addEventListener("click", fn); return b; };
  if (!isNew) {
    act.append(btn("Quitar del mapa", "ghost danger", async () => {
      state.map = await invoke("artist_remove", { id: a.id });
      mapUI.editing = null; renderMap(); refreshSuggestions();
      toast(`${a.name} quedó en Descartados`);
    }));
    act.append(btn("Medir ahora", "ghost", () => measureIds([a.id])));
  }
  act.append(el("span", "grow"));
  act.append(btn("Cancelar", "ghost", () => { mapUI.editing = null; renderMap(); }));
  act.append(btn("Guardar", "accent", async () => {
    const num = (x) => parseInt(x.value, 10) || 0;
    const artist = { ...a, name: name.value, aliases: aliases.value.split(",").map((x) => x.trim()).filter(Boolean), country: country.value, city: city.value.trim(), group: group.value, style: style.value.trim(), vibe: vibe.value.trim(), bpm_min: num(bmin), bpm_max: num(bmax) || num(bmin), level: level.value };
    try {
      state.map = await invoke("artist_save", { artist });
      mapUI.editing = null; renderMap(); refreshSuggestions();
    } catch (e) { toast(String(e)); }
  }));
  f.append(act);
  f.addEventListener("submit", (e) => e.preventDefault());
  setTimeout(() => name.focus(), 0);
  return f;
}

$$("#mapCountry button").forEach((b) => b.addEventListener("click", () => { mapUI.country = b.dataset.c; renderMap(); }));
$("#mapSearch").addEventListener("input", (e) => { mapUI.q = e.target.value; renderMap(); });
$("#mapAdd").addEventListener("click", () => { mapUI.editing = ""; renderMap(); });
$("#mapSync").addEventListener("click", async () => {
  const b = $("#mapSync");
  b.disabled = true; b.textContent = "Leyendo…";
  try {
    state.map = await invoke("artists_sync");
    renderMap(); refreshSuggestions();
    toast(`Leí ${state.map.total_requests} pedidos`);
  } catch (e) { toast(String(e)); }
  b.disabled = false; b.textContent = "Leer pedidos de Notion";
});
$("#mapMeasure").addEventListener("click", () => measureIds([]));

async function measureIds(ids) {
  const b = $("#mapMeasure");
  b.disabled = true; b.textContent = "Midiendo…";
  try {
    const msg = await invoke("artists_measure", { ids });
    toast(msg);
  } catch (e) { toast(String(e)); }
  b.disabled = false; b.textContent = "Medir oportunidad";
  await loadMap();
  refreshSuggestions();
}

/* ---------- artista de referencia: comparación y sugeridos ---------- */

const GENRE_GROUP = { "R&B": "rnb", Soul: "rnb", Newjazz: "rnb", POP: "rnb", "Reggaetón": "rnb", Afro: "rnb", "Hip-Hop": "rap", Trap: "trap", Plugg: "trap", Y2K: "trap" };

function findArtist(name) {
  const f = (x) => x.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase().trim();
  const n = f(name);
  return state.map?.artists.find((a) => f(a.name) === n || a.aliases.some((x) => f(x) === n));
}

function renderArtistPop() {
  const pop = $("#artPop");
  const names = $("#f-artist").value.split(/\s+x\s+|,|&/i).map((x) => x.trim()).filter((x) => x.length >= 2).slice(0, 4);
  pop.innerHTML = "";
  pop.hidden = !names.length || document.activeElement !== $("#f-artist") && !pop.matches(":hover");
  for (const name of names) {
    const a = findArtist(name);
    const r = el("div", "ap-row");
    const n = el("div", "ap-n");
    n.append(el("b", null, a ? a.name : name), el("small", null, a ? [COUNTRY[a.country], a.style].filter(Boolean).join(" · ") : "No está en el mapa"));
    const o = a?.opp || state.oppCache?.[name.toLowerCase()];
    r.append(n, barsEl(o));
    if (o) r.append(el("p", "ap-why", `${o.label}. ${o.why}`));
    else {
      const b = el("button", "link", "Medir");
      b.type = "button";
      b.addEventListener("mousedown", (e) => e.preventDefault());
      b.addEventListener("click", async () => {
        b.textContent = "Midiendo…";
        try {
          const res = await invoke("artist_opportunity", { name });
          state.oppCache = { ...(state.oppCache || {}), [name.toLowerCase()]: res };
          if (a) await loadMapQuiet();
        } catch (e) { toast(String(e)); }
        renderArtistPop();
      });
      r.append(b);
    }
    pop.append(r);
  }
}

async function loadMapQuiet() { try { state.map = await invoke("artists_get"); } catch {} }

$("#f-artist").addEventListener("input", renderArtistPop);
$("#f-artist").addEventListener("focus", renderArtistPop);
$("#f-artist").addEventListener("blur", () => setTimeout(() => { if (!$("#artPop").matches(":hover")) $("#artPop").hidden = true; }, 150));

function refreshSuggestions() {
  const card = $("#sugCard");
  if (!state.map) { card.hidden = true; return; }
  const input = readInput();
  const groups = new Set(input.generos.map((g) => GENRE_GROUP[g]).filter(Boolean));
  if (!groups.size) groups.add("rnb");
  const bpm = parseInt(input.bpm, 10) || 0;
  const fits = (a) => !bpm || !a.bpm_min || [bpm, bpm * 2, Math.round(bpm / 2)].some((x) => x >= a.bpm_min - 5 && x <= (a.bpm_max || a.bpm_min) + 5);
  const used = new Set($("#f-artist").value.split(/\s+x\s+|,/i).map((x) => x.trim().toLowerCase()));
  const list = state.map.artists.filter((a) => groups.has(a.group) && fits(a) && !/grande/i.test(a.level) && !used.has(a.name.toLowerCase())).sort(rankArtists).slice(0, 6);
  card.hidden = !list.length;
  const label = [...groups].map((g) => GROUPS.find((x) => x[0] === g)?.[1]).join(" · ");
  $("#sugLabel").textContent = `Sugeridos para ${label}${bpm ? ` · ${bpm} BPM` : ""}`;
  const box = $("#artSug");
  box.innerHTML = "";
  for (const a of list) {
    const c = el("button", `pill o${a.opp ? a.opp.level : 0}`, a.name);
    c.type = "button";
    c.title = a.opp ? `${a.opp.label}. ${a.opp.why}` : `${a.style}. Sin medir todavía.`;
    c.addEventListener("click", () => {
      const f = $("#f-artist");
      const cur = f.value.split(/\s+x\s+/i).map((x) => x.trim()).filter(Boolean);
      f.value = (cur.length >= 1 && cur.length < 2 ? [...cur, a.name] : [a.name]).join(" x ");
      changed(); refreshSuggestions();
    });
    box.append(c);
  }
}

/* ---------- inicio ---------- */

async function main() {
  const key = $("#f-key");
  for (const k of KEYS) key.add(new Option(k, k));
  buildChips("#c-genero", GENEROS, "genero");
  buildChips("#c-mood", MOODS, "mood");
  buildChips("#c-instr", INSTR, "instr", 3);
  $("#f-bpm").addEventListener("input", (e) => (e.target.value = e.target.value.replace(/\D/g, "")));
  $$("#beatForm input, #beatForm select").forEach((e) => e.addEventListener("input", changed));
  $("#beatForm").addEventListener("submit", (e) => e.preventDefault());
  $$("#ytSeg button").forEach((b) => b.addEventListener("click", () => { setPrivacy(b.dataset.p); changed(); }));
  $("#ytTz").textContent = `hora de tu PC (${Intl.DateTimeFormat().resolvedOptions().timeZone.split("/").pop().replace(/_/g, " ")})`;

  await loadSettings();
  writeInput(store.get("borrador") || {});
  setPreview("yt");
  refreshPreview();
  renderPublish();
  await renderRecent();
  await loadMapQuiet();
  refreshSuggestions();
  await initDragDrop();
  await T.event.listen("oportunidad", (e) => {
    const a = state.map?.artists.find((x) => x.id === e.payload.id);
    if (a) { a.opp = e.payload.opp; if (state.view === "mapa") renderMap(); }
  });

  await T.event.listen("trabajo", (e) => {
    if (state.job && e.payload.handle === state.job.handle) { state.job = e.payload; renderJob(); }
    if (!e.payload.running) renderRecent();
  });
  await T.event.listen("avance", (e) => onProgress(e.payload));
  await T.event.listen("actualizacion", (e) => { $("#updInstall").textContent = `Descargando ${Math.round(e.payload * 100)}%`; });
  setTimeout(() => checkUpdate(false), 2500);

  const s = state.settings;
  if (!state.view?.has_tienda || !s.r2_account_id || !s.r2_access_key_id) toast("Falta configurar las conexiones en Ajustes");
  $("#f-name").focus();
}

main();
