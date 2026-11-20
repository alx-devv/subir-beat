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

const STEP_TITLES = { archivos: "Zips", bucket: "Bucket", web: "Imagen de la tienda", shopify: "Producto", publicar: "Publicación", video: "Video", youtube: "YouTube" };
const STEP_ORDER = ["archivos", "bucket", "web", "shopify", "publicar", "video", "youtube"];
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
  for (const v of ["nuevo", "trabajo", "ajustes"]) $(`#view-${v}`).hidden = v !== view;
  $("#settingsBtn").classList.toggle("on", view === "ajustes");
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
  if (!jobs.length) { ul.append(el("li", "empty", "Todavía no hay beats")); return; }
  for (const j of jobs) {
    const done = STEP_ORDER.every((s) => j.steps?.[s]?.status === "listo");
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
    price_premium: $("#f-pp").value.trim(),
    price_exclusiva: $("#f-pe").value.trim(),
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
  state.privacy = ["public", "unlisted", "scheduled"].includes(p) ? p : "public";
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
  $("#f-pp").value = d.price_premium || state.settings?.price_premium || "49";
  $("#f-pe").value = d.price_exclusiva || state.settings?.price_exclusiva || "125";
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
  row.classList.toggle("filled", !!path);
  const v = $(".fval", row);
  v.textContent = path ? baseName(path) : v.dataset.empty;
  if (slot === "mp3") loadAudio(path);
  if (slot === "thumbnail") loadThumbInfo(path);
  if (slot === "wav" && path && !silent) detectFromWav(path);
  if (slot === "visual") stopPreview();
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
    const fits = (s) => ({ wav: f.ext === "wav", mp3: f.ext === "mp3", stems: f.is_dir, visual: VISUAL_EXT.includes(f.ext), thumbnail: IMAGE_EXT.includes(f.ext) }[s]);
    let slot = null;
    if (target && fits(target)) slot = target;
    else if (f.is_dir) slot = "stems";
    else if (f.ext === "wav") slot = "wav";
    else if (f.ext === "mp3") slot = "mp3";
    else if (IMAGE_EXT.includes(f.ext) && f.width && Math.abs(f.width / f.height - 16 / 9) < 0.03) slot = "thumbnail";
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
  visual: [{ name: "Imagen, gif o video", extensions: VISUAL_EXT }],
  thumbnail: [{ name: "Imagen", extensions: IMAGE_EXT }],
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
  $("#stVarP").textContent = `WAV Premium — $${money(input.price_premium)} USD`;
  $("#stVarE").textContent = `WAV + STEMS Exclusiva — $${money(input.price_exclusiva)} USD`;
  const vis = state.slots.visual;
  metaRows($("#stMeta"), [
    ["Imagen", !vis ? "—" : VIDEO_EXT.includes(ext(vis)) ? `gif ${state.settings?.gif_size || 600} × ${state.settings?.gif_size || 600} · se crea al publicar` : ext(vis) === "gif" ? "gif tal cual" : "imagen tal cual"],
    ["Link", p.handle ? `/products/${p.handle}` : "—"],
    ["Canales", "Tienda · Instagram · POS"],
  ]);
}, 150);

/* ---------- publicar ---------- */

function markInvalid(msg) {
  const map = [["nombre", "#f-name"], ["BPM", "#f-bpm"], ["tonalidad", "#f-key"], ["artista", "#f-artist"], ["precios", "#f-pp"]];
  for (const [w, sel] of map) if (msg.includes(w)) $(sel).classList.add("invalid");
  for (const [w, slot] of [["WAV", "wav"], ["stems", "stems"], ["mp3", "mp3"], ["visual", "visual"]]) {
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
  $('[data-secret="shopify"]').placeholder = v.has_shopify_secret ? "Guardado (escribí para cambiarlo)" : "Sin cargar";
  $('[data-secret="r2"]').placeholder = v.has_r2_secret ? "Guardado (escribí para cambiarlo)" : "Sin cargar";
  $('[data-secret="yt"]').placeholder = v.has_yt_secret ? "Guardado (escribí para cambiarlo)" : "Sin cargar";
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
    await invoke("save_settings", { settings: s, shopifySecret: $('[data-secret="shopify"]').value, r2Secret: $('[data-secret="r2"]').value, ytSecret: $('[data-secret="yt"]').value });
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
  for (const [name, c] of [["Shopify", r.shopify], ["R2", r.r2], ["ffmpeg", r.ffmpeg]]) {
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
  await initDragDrop();

  await T.event.listen("trabajo", (e) => {
    if (state.job && e.payload.handle === state.job.handle) { state.job = e.payload; renderJob(); }
    if (!e.payload.running) renderRecent();
  });
  await T.event.listen("avance", (e) => onProgress(e.payload));
  await T.event.listen("actualizacion", (e) => { $("#updInstall").textContent = `Descargando ${Math.round(e.payload * 100)}%`; });
  setTimeout(() => checkUpdate(false), 2500);

  const s = state.settings;
  if (!s.shop_client_id || !s.r2_account_id || !s.r2_access_key_id) toast("Falta configurar las conexiones en Ajustes");
  $("#f-name").focus();
}

main();
