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

const STEP_TITLES = {
  archivos: "Zips",
  bucket: "Bucket",
  web: "Imagen de la tienda",
  shopify: "Producto",
  publicar: "Publicación",
  video: "Video",
  youtube: "YouTube",
};
const STEP_ORDER = ["archivos", "bucket", "web", "shopify", "publicar", "video", "youtube"];

const VISUAL_EXT = ["jpg", "jpeg", "png", "webp", "gif", "mp4", "mov", "webm", "mkv", "m4v"];
const IMAGE_EXT = ["jpg", "jpeg", "png", "webp"];

const state = {
  slots: { visual: "", thumbnail: "", wav: "", stems: "", mp3: "" },
  chips: { genero: [], mood: [], instr: [] },
  settings: null,
  job: null,
};

/* ---------- utilidades ---------- */

let toastTimer;
function toast(msg) {
  const t = $("#toast");
  t.textContent = msg;
  t.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove("show"), 2200);
}

function baseName(p) {
  return p.split(/[\\/]/).pop();
}

function debounce(fn, ms) {
  let t;
  return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); };
}

const store = {
  get(k) { try { return JSON.parse(localStorage.getItem(k)); } catch { return null; } },
  set(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); } catch { /* sin almacenamiento */ } },
  del(k) { try { localStorage.removeItem(k); } catch { /* nada */ } },
};

/* ---------- navegación ---------- */

function show(view) {
  $$(".view").forEach((v) => (v.hidden = v.id !== `view-${view}`));
  $$(".tab").forEach((t) => t.classList.toggle("is-on", t.dataset.view === view));
  if (view === "historial") renderHistory();
  if (view === "ajustes") loadSettingsForm();
}
$$(".tab").forEach((t) => t.addEventListener("click", () => show(t.dataset.view)));
$("#setupHint").addEventListener("click", () => show("ajustes"));

/* ---------- formulario ---------- */

function buildChips(id, list, group, max) {
  const box = $(id);
  box.innerHTML = "";
  for (const v of list) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "chip";
    b.textContent = v;
    b.setAttribute("aria-pressed", "false");
    b.addEventListener("click", () => {
      const arr = state.chips[group];
      const i = arr.indexOf(v);
      if (i >= 0) arr.splice(i, 1);
      else if (!max || arr.length < max) arr.push(v);
      syncChips();
      changed();
    });
    box.appendChild(b);
  }
}

function syncChips() {
  for (const [group, id, max] of [["genero", "#c-genero"], ["mood", "#c-mood"], ["instr", "#c-instr", 3]]) {
    const sel = state.chips[group];
    $$(".chip", $(id)).forEach((b) => {
      const on = sel.includes(b.textContent);
      b.setAttribute("aria-pressed", String(on));
      b.disabled = !!max && !on && sel.length >= max;
    });
  }
}

function initForm() {
  const key = $("#f-key");
  for (const k of KEYS) key.add(new Option(k, k));
  buildChips("#c-genero", GENEROS, "genero");
  buildChips("#c-mood", MOODS, "mood");
  buildChips("#c-instr", INSTR, "instr", 3);
  $("#f-bpm").addEventListener("input", (e) => (e.target.value = e.target.value.replace(/\D/g, "")));
  $$("#beatForm input, #beatForm select").forEach((el) => el.addEventListener("input", changed));
  $("#beatForm").addEventListener("submit", (e) => { e.preventDefault(); publish(); });
  $("#clearBtn").addEventListener("click", clearForm);
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
    wav: state.slots.wav,
    stems: state.slots.stems,
    mp3: state.slots.mp3,
    visual: state.slots.visual,
    thumbnail: state.slots.thumbnail,
  };
}

function writeInput(d) {
  $("#f-name").value = d.name || "";
  $("#f-bpm").value = d.bpm || "";
  $("#f-key").value = d.key || "";
  $("#f-artist").value = d.artist || "";
  $("#f-pp").value = d.price_premium || state.settings?.price_premium || "49";
  $("#f-pe").value = d.price_exclusiva || state.settings?.price_exclusiva || "125";
  state.chips.genero = d.generos || [];
  state.chips.mood = d.moods || [];
  state.chips.instr = d.instruments || [];
  for (const k of Object.keys(state.slots)) setSlot(k, d[k] || "", true);
  syncChips();
}

const refreshPreview = debounce(async () => {
  const input = readInput();
  if (!input.name) { $("#names").hidden = true; return; }
  const p = await invoke("preview", { input });
  $("#n-folder").textContent = p.folder;
  $("#n-premium").textContent = p.premium;
  $("#n-exclusiva").textContent = p.exclusiva;
  $("#n-title").textContent = p.title;
  $("#names").hidden = false;
}, 180);

function changed() {
  $("#formMsg").textContent = "";
  $$(".invalid").forEach((el) => el.classList.remove("invalid"));
  store.set("borrador", readInput());
  refreshPreview();
}

function clearForm() {
  store.del("borrador");
  writeInput({});
  changed();
  $("#f-name").focus();
}

/* ---------- archivos ---------- */

function setSlot(slot, path, silent) {
  state.slots[slot] = path;
  const el = $(`.drop[data-slot="${slot}"]`);
  el.classList.toggle("filled", !!path);
  if (slot === "visual" || slot === "thumbnail") {
    const media = $(".drop-media", el);
    media.innerHTML = "";
    if (path) {
      const ext = path.split(".").pop().toLowerCase();
      let node;
      if (["mp4", "mov", "webm", "mkv", "m4v"].includes(ext)) {
        node = document.createElement("video");
        Object.assign(node, { autoplay: true, loop: true, muted: true, playsInline: true });
      } else {
        node = document.createElement("img");
        node.alt = "";
      }
      node.src = src(path);
      media.appendChild(node);
    }
    $(".drop-name", el).textContent = path ? baseName(path) : "";
  } else {
    const empty = { wav: "Soltá el .wav", stems: "Soltá la carpeta", mp3: "Soltá el .mp3" }[slot];
    $(".slot-val", el).textContent = path ? baseName(path) : empty;
  }
  if (!silent) changed();
}

/** Decide dónde va cada cosa arrastrada (o elegida), aunque no caiga justo en su lugar. */
async function place(paths, targetSlot) {
  const infos = await invoke("path_info", { paths });
  const unplaced = [];
  for (const f of infos) {
    let slot = null;
    const fits = (s) => {
      if (s === "wav") return f.ext === "wav";
      if (s === "mp3") return f.ext === "mp3";
      if (s === "stems") return f.is_dir;
      if (s === "visual") return VISUAL_EXT.includes(f.ext);
      if (s === "thumbnail") return IMAGE_EXT.includes(f.ext);
      return false;
    };
    if (targetSlot && fits(targetSlot)) slot = targetSlot;
    else if (f.is_dir) slot = "stems";
    else if (f.ext === "wav") slot = "wav";
    else if (f.ext === "mp3") slot = "mp3";
    else if (IMAGE_EXT.includes(f.ext) && f.width && Math.abs(f.width / f.height - 16 / 9) < 0.03) slot = "thumbnail";
    else if (VISUAL_EXT.includes(f.ext)) slot = "visual";
    if (slot) {
      setSlot(slot, f.path);
      if (slot === "stems" && f.wav_count === 0) toast("Ojo: esa carpeta no tiene archivos .wav");
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

$$(".drop").forEach((el) =>
  el.addEventListener("click", async () => {
    const slot = el.dataset.slot;
    const picked = await T.dialog.open({
      directory: slot === "stems",
      multiple: false,
      filters: FILTERS[slot],
    });
    if (picked) place([picked], slot);
  })
);

function zoneAt(x, y) {
  const el = document.elementFromPoint(x, y);
  return el ? el.closest(".drop") : null;
}

async function initDragDrop() {
  let over = null;
  await T.webview.getCurrentWebview().onDragDropEvent((ev) => {
    const p = ev.payload;
    const dpr = window.devicePixelRatio || 1;
    if (p.type === "enter" || p.type === "over") {
      document.body.classList.add("dragging");
      const z = p.position ? zoneAt(p.position.x / dpr, p.position.y / dpr) : null;
      if (z !== over) { over?.classList.remove("over"); z?.classList.add("over"); over = z; }
    } else {
      document.body.classList.remove("dragging");
      over?.classList.remove("over");
      if (p.type === "drop" && p.paths?.length && !$("#view-nuevo").hidden) {
        place(p.paths, over?.dataset.slot);
      }
      over = null;
    }
  });
}

/* ---------- publicar ---------- */

function markInvalid(msg) {
  const map = [
    ["nombre", "#f-name"], ["BPM", "#f-bpm"], ["tonalidad", "#f-key"],
    ["artista", "#f-artist"], ["precios", "#f-pp"],
  ];
  for (const [word, sel] of map) if (msg.includes(word)) $(sel).classList.add("invalid");
  const slotWords = [["WAV", "wav"], ["stems", "stems"], ["mp3", "mp3"], ["visual", "visual"]];
  for (const [word, slot] of slotWords) if (msg.includes(word)) $(`.drop[data-slot="${slot}"]`).classList.add("over");
  setTimeout(() => $$(".drop.over").forEach((d) => d.classList.remove("over")), 1600);
}

async function publish() {
  const input = readInput();
  const btn = $("#publishBtn");
  try {
    await invoke("validate_input", { input });
  } catch (e) {
    $("#formMsg").textContent = e;
    markInvalid(String(e));
    return;
  }
  btn.disabled = true;
  try {
    const handle = await invoke("start_job", { input });
    store.del("borrador");
    openJob({ handle, input, steps: {}, running: true });
  } catch (e) {
    $("#formMsg").textContent = e;
  } finally {
    btn.disabled = false;
  }
}

/* ---------- vista del trabajo ---------- */

function openJob(job) {
  state.job = job;
  renderJob();
  show("trabajo");
}

function renderJob() {
  const j = state.job;
  if (!j) return;
  $("#jobName").textContent = j.input.name;
  const done = STEP_ORDER.every((s) => j.steps?.[s]?.status === "listo");
  $("#jobSub").textContent = j.running
    ? "Subiendo. Podés dejar la app abierta y seguir con otra cosa."
    : done
      ? "Listo. Falta solo subir el video en YouTube Studio."
      : j.error ? "Se detuvo antes de terminar." : "";

  const ol = $("#steps");
  if (!ol.children.length) {
    for (const s of STEP_ORDER) {
      const li = document.createElement("li");
      li.className = "step";
      li.dataset.step = s;
      li.innerHTML = `<span class="dot"></span><span class="step-title">${STEP_TITLES[s]}</span><span class="step-detail"><span class="txt"></span><span class="meter" hidden><i></i></span></span>`;
      ol.appendChild(li);
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

  const showErr = !j.running && !!j.error;
  $("#jobError").hidden = !showErr;
  $("#jobErrorText").textContent = j.error || "";

  $("#result").hidden = !done;
  if (done) {
    $("#ytTitle").textContent = j.yt_title;
    $("#ytDesc").textContent = j.yt_description;
    $("#ytTags").textContent = j.yt_tags;
    const img = $("#ytThumb");
    img.hidden = !j.thumbnail;
    if (j.thumbnail) img.src = src(j.thumbnail) + `?v=${j.created}`;
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

$("#jobBack").addEventListener("click", () => {
  $("#steps").innerHTML = "";
  show("nuevo");
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
$("#revealVideo").addEventListener("click", () => {
  if (state.job?.video) T.opener.revealItemInDir(state.job.video);
});
$("#openStudio").addEventListener("click", async () => {
  // Título copiado de entrada: es lo primero que pide Studio después de soltar el video.
  if (state.job?.yt_title) await T.clipboardManager.writeText(state.job.yt_title);
  if (state.job?.video) T.opener.revealItemInDir(state.job.video);
  T.opener.openUrl("https://www.youtube.com/upload");
  toast("Título copiado. Arrastrá el video a Studio.");
});

/* ---------- historial ---------- */

function fmtDate(secs) {
  return new Date(secs * 1000).toLocaleString("es-AR", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
}

async function renderHistory() {
  const jobs = await invoke("list_jobs");
  const ul = $("#history");
  ul.innerHTML = "";
  $("#historyEmpty").hidden = jobs.length > 0;
  for (const j of jobs) {
    const done = STEP_ORDER.every((s) => j.steps?.[s]?.status === "listo");
    const li = document.createElement("li");
    const name = document.createElement("button");
    name.className = "h-name";
    name.textContent = j.input.name;
    name.addEventListener("click", () => { $("#steps").innerHTML = ""; openJob(j); });
    const date = document.createElement("span");
    date.className = "h-date";
    date.textContent = fmtDate(j.created);
    const st = document.createElement("span");
    st.className = "h-state " + (done ? "ok" : j.running ? "" : "bad");
    st.textContent = done ? "Publicado" : j.running ? "Subiendo" : "A medias";
    li.append(name, date, st);
    ul.appendChild(li);
  }
}

/* ---------- ajustes ---------- */

async function loadSettings() {
  const v = await invoke("get_settings");
  state.settings = v.settings;
  const s = v.settings;
  const missing = !v.has_shopify_secret || !v.has_r2_secret || !s.shop_client_id || !s.r2_account_id || !s.r2_access_key_id;
  $("#setupHint").hidden = !missing;
  return v;
}

async function loadSettingsForm() {
  const v = await loadSettings();
  const f = $("#settingsForm");
  for (const [k, val] of Object.entries(v.settings)) {
    const el = f.elements[k];
    if (!el) continue;
    if (el.type === "checkbox") el.checked = !!val;
    else el.value = val;
  }
  $('[data-secret="shopify"]').placeholder = v.has_shopify_secret ? "Guardado (escribí para cambiarlo)" : "Sin cargar";
  $('[data-secret="r2"]').placeholder = v.has_r2_secret ? "Guardado (escribí para cambiarlo)" : "Sin cargar";
  $$("[data-secret]").forEach((el) => (el.value = ""));
}

$("#settingsForm").addEventListener("submit", async (e) => {
  e.preventDefault();
  const f = e.target;
  const s = { ...state.settings };
  for (const k of Object.keys(s)) {
    const el = f.elements[k];
    if (!el) continue;
    if (el.type === "checkbox") s[k] = el.checked;
    else if (typeof s[k] === "number") s[k] = parseInt(el.value, 10) || s[k];
    else s[k] = el.value.trim();
  }
  try {
    await invoke("save_settings", {
      settings: s,
      shopifySecret: $('[data-secret="shopify"]').value,
      r2Secret: $('[data-secret="r2"]').value,
    });
    await loadSettingsForm();
    toast("Ajustes guardados");
  } catch (err) { toast(String(err)); }
});

$("#testBtn").addEventListener("click", async () => {
  const ul = $("#checks");
  ul.innerHTML = "<li>Probando…</li>";
  const r = await invoke("test_connections");
  ul.innerHTML = "";
  for (const [name, c] of [["Shopify", r.shopify], ["R2", r.r2], ["ffmpeg", r.ffmpeg]]) {
    const li = document.createElement("li");
    li.className = c.ok ? "ok" : "";
    li.textContent = `${name}: ${c.detail}`;
    li.title = c.detail;
    ul.appendChild(li);
  }
});

$("#pickOut").addEventListener("click", async () => {
  const p = await T.dialog.open({ directory: true });
  if (p) $("#s-out").value = p;
});

/* ---------- inicio ---------- */

async function main() {
  initForm();
  await loadSettings();
  writeInput(store.get("borrador") || {});
  refreshPreview();
  await initDragDrop();

  await T.event.listen("trabajo", (e) => {
    if (state.job && e.payload.handle === state.job.handle) {
      state.job = e.payload;
      renderJob();
    }
  });
  await T.event.listen("avance", (e) => onProgress(e.payload));

  // Si quedó algo a medias, se ofrece retomarlo.
  const jobs = await invoke("list_jobs");
  const pending = jobs.find((j) => !j.running && j.error);
  if (pending) toast(`"${pending.input.name}" quedó a medias. Está en Historial.`);
  $("#f-name").focus();
}

main();
