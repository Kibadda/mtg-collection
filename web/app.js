"use strict";

const MIN_BATCH = 75;
const SCRYFALL_COLLECTION = "https://api.scryfall.com/cards/collection";

const state = {
  entries: [],
  cache: new Map(),
  filters: {
    search: "", finish: "all", condition: "all", set: "all", rarity: "all",
    lang: "all", color: "all", cmcMin: "", cmcMax: "", priceMin: "",
    priceMax: "", copies: "",
  },
  sort: "name",
  dir: 1,
};

const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
}[c]));

const keyOf = (set, number) => `${set}#${number}`;

function artUrl(data) {
  if (!data) return null;
  const uris = data.image_uris || (data.card_faces && data.card_faces[0] && data.card_faces[0].image_uris);
  if (!uris) return null;
  return uris.large || uris.normal || uris.small || uris.png || null;
}

function priceOf(entry) {
  const p = entry.data && entry.data.prices;
  if (!p) return null;
  const v = entry.card.finish === "foil" ? p.eur_foil : p.eur;
  return v !== null && v !== undefined ? v : null;
}

const SORTS = {
  name: ["Name", (a, b) => a.card.name.localeCompare(b.card.name)],
  set: ["Set + number", (a, b) =>
    keyOf(a.card.set, a.card.collector_number).localeCompare(
      keyOf(b.card.set, b.card.collector_number), undefined, { numeric: true })],
  price: ["Price", (a, b) => (Number(priceOf(a)) || 0) - (Number(priceOf(b)) || 0)],
  qty: ["Quantity", (a, b) => a.card.quantity - b.card.quantity],
};

function fillSelect(id, placeholder, label, values, current) {
  const sel = document.getElementById(id);
  sel.innerHTML = `<option value="${placeholder}">${label}</option>` +
    values.map((v) => `<option value="${esc(v)}">${esc(v)}</option>`).join("");
  if ([...sel.options].some((o) => o.value === current)) sel.value = current;
}

function populateFilterOptions() {
  const sets = new Set(), rarities = new Set(), langs = new Set();
  for (const { card, data } of state.entries) {
    sets.add(card.set);
    langs.add(card.lang || "en");
    if (data && data.rarity) rarities.add(data.rarity);
  }
  fillSelect("f-set", "all", "Set: all", [...sets].sort(), state.filters.set);
  fillSelect("f-rarity", "all", "Rarity: all", [...rarities].sort(), state.filters.rarity);
  fillSelect("f-lang", "all", "Language: all", [...langs].sort(), state.filters.lang);

  const sortSel = document.getElementById("f-sort");
  sortSel.innerHTML = Object.entries(SORTS)
    .map(([id, [label]]) => `<option value="${id}">Sort: ${label}</option>`).join("");
  sortSel.value = state.sort;
}

function readFilters() {
  const g = (id) => document.getElementById(id).value;
  state.filters = {
    search: g("f-search").trim().toLowerCase(),
    finish: g("f-finish"),
    condition: g("f-condition"),
    set: g("f-set"),
    rarity: g("f-rarity"),
    lang: g("f-lang"),
    color: g("f-color"),
    cmcMin: g("f-cmc-min"),
    cmcMax: g("f-cmc-max"),
    priceMin: g("f-price-min"),
    priceMax: g("f-price-max"),
    copies: g("f-copies"),
  };
}

function matches(entry) {
  const f = state.filters, { card, data } = entry;
  if (f.search) {
    const hay = [
      card.name, card.set, (data && data.set_name) || "",
      (data && data.oracle_text) || "", (data && data.type_line) || "",
    ].join(" ").toLowerCase();
    if (!hay.includes(f.search)) return false;
  }
  if (f.finish !== "all" && card.finish !== f.finish) return false;
  if (f.condition !== "all" && card.condition !== f.condition) return false;
  if (f.set !== "all" && card.set !== f.set) return false;
  if (f.rarity !== "all" && data && data.rarity !== f.rarity) return false;
  if (f.rarity !== "all" && !data) return false;
  if (f.lang !== "all" && (card.lang || "en") !== f.lang) return false;
  const identity = (data && data.color_identity) || [];
  if (f.color !== "all") {
    if (f.color === "C" ? identity.length !== 0 : !identity.includes(f.color)) return false;
  }
  const cmc = data && data.cmc;
  if (f.cmcMin !== "" && (cmc === undefined || cmc < Number(f.cmcMin))) return false;
  if (f.cmcMax !== "" && (cmc === undefined || cmc > Number(f.cmcMax))) return false;
  const price = priceOf(entry);
  if (f.priceMin !== "" && (price === null || Number(price) < Number(f.priceMin))) return false;
  if (f.priceMax !== "" && (price === null || Number(price) > Number(f.priceMax))) return false;
  if (f.copies !== "" && card.quantity < Number(f.copies)) return false;
  return true;
}

function artHtml(entry) {
  const { card, data } = entry;
  const url = artUrl(data);
  const qty = `<span class="badge qty">×${card.quantity}</span>`;
  const foil = card.finish === "foil"
    ? `<span class="badge foil">FOIL</span><span class="foil-shimmer"></span>` : "";
  const lang = card.lang && card.lang !== "en"
    ? `<span class="badge lang">${esc(card.lang.toUpperCase())}</span>` : "";
  if (url) {
    return `<div class="art">
      <img src="${esc(url)}" alt="${esc(card.name)}" loading="lazy">
      ${foil}${qty}${lang}
    </div>`;
  }
  if (!data) {
    return `<div class="art art-ph">
      <span class="ph-title">No art</span>
      <span class="ph-hint">${card.collector_number
        ? "not_found on Scryfall" : "old-format card"}</span>
      <span class="ph-hint">remove &amp; re-add via CLI</span>
      ${qty}${foil}${lang}
    </div>`;
  }
  return `<div class="art art-ph">
    <span class="ph-title">${esc(card.name)}</span>
    <span class="ph-hint">no art available</span>
    ${qty}${foil}${lang}
  </div>`;
}

function metaHtml(entry) {
  const { card, data } = entry;
  const setNum = card.collector_number
    ? `${esc(card.set.toUpperCase())} #${esc(card.collector_number)}`
    : esc(card.set.toUpperCase());
  const price = priceOf(entry);
  return `<div class="meta">
    <div class="name" title="${esc(card.name)}">${esc(card.name)}${data ? "" : " *"}</div>
    <div class="sub">${setNum}</div>
    <div class="row3">
      <span class="cond">${esc(card.condition)}</span>
      ${price !== null ? `<span class="price">€${esc(price)}</span>` : ""}
    </div>
  </div>`;
}

function tile(entry) {
  const el = document.createElement("article");
  el.className = "tile" + (entry.data ? "" : " tile-unresolved");
  el.innerHTML = artHtml(entry) + metaHtml(entry);
  el.addEventListener("click", () => openModal(entry));
  return el;
}

function openModal(entry) {
  const { card, data } = entry;
  const body = document.getElementById("modal-body");
  let html = `<div class="modal-flex">`;
  const url = artUrl(data);
  if (url) html += `<img class="modal-art" src="${esc(url)}" alt="">`;
  html += `<div class="modal-detail"><h2>${esc(card.name)}</h2>`;
  html += `<div class="badges">
    <span class="badge ${card.finish === "foil" ? "foil" : "nf"}">${card.finish === "foil" ? "FOIL" : "NONFOIL"}</span>
    <span class="badge cond">${esc(card.condition)}</span>
    ${card.lang && card.lang !== "en" ? `<span class="badge lang">${esc(card.lang.toUpperCase())}</span>` : ""}
    <span class="badge qty">×${card.quantity}</span>
  </div>`;

  if (data) {
    const cost = data.mana_cost ? `  ${esc(data.mana_cost)}` : "";
    html += `<p class="type">${esc(data.type_line)}${cost}</p>`;
    if (data.oracle_text) html += `<p class="oracle">${esc(data.oracle_text)}</p>`;

    const p = data.prices || {};
    const priceRows = [
      ["EUR (nonfoil)", p.eur], ["EUR (foil)", p.eur_foil],
    ].filter(([, v]) => v);
    if (priceRows.length) {
      html += `<h3>Prices</h3><table class="prices">`;
      for (const [label, v] of priceRows) {
        const hl = (v === (card.finish === "foil" ? p.eur_foil : p.eur));
        html += `<tr${hl ? ' class="highlight"' : ""}><td>${label}</td><td class="amount">€${esc(v)}</td></tr>`;
      }
      html += `</table>`;
    }

    html += `<h3>Print info</h3><table class="print">
      <tr><td>Set</td><td>${esc(data.set_name || "")} (${esc(data.set.toUpperCase())})</td></tr>
      ${data.released_at ? `<tr><td>Released</td><td>${esc(data.released_at)}</td></tr>` : ""}
      <tr><td>Rarity</td><td>${esc(data.rarity)}</td></tr>
      <tr><td>Number</td><td>${esc(data.collector_number)}</td></tr>
    </table>`;
  } else {
    html += `<p class="hint">Details unavailable — remove &amp; re-add this card via the CLI to restore art, text and prices.</p>`;
  }

  html += `</div></div>`;
  body.innerHTML = html;
  document.getElementById("modal").classList.remove("hidden");
}

function closeModal() {
  document.getElementById("modal").classList.add("hidden");
}

function render() {
  readFilters();
  const shown = state.entries.filter(matches).sort((a, b) => {
    const cmp = SORTS[state.sort][1](a, b);
    return cmp !== 0 ? cmp * state.dir : a.card.name.localeCompare(b.card.name);
  });
  const grid = document.getElementById("grid");
  grid.replaceChildren();
  if (shown.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = "No cards match the current filters.";
    grid.appendChild(empty);
  } else {
    shown.forEach((e) => grid.appendChild(tile(e)));
  }
  const total = state.entries.reduce((s, e) => s + e.card.quantity, 0);
  const shownQty = shown.reduce((s, e) => s + e.card.quantity, 0);
  const unresolved = state.entries.filter((e) => !e.data).length;
  document.getElementById("summary").textContent =
    `${shown.length} of ${state.entries.length} entries · ${shownQty} copies shown · ` +
    (`${total} copies total` + (unresolved ? ` · ${unresolved} unresolved` : ""));
}

function resetFilters() {
  for (const [id, val] of Object.entries({
    "f-search": "", "f-finish": "all", "f-condition": "all", "f-set": "all",
    "f-rarity": "all", "f-lang": "all", "f-color": "all", "f-cmc-min": "",
    "f-cmc-max": "", "f-price-min": "", "f-price-max": "", "f-copies": "",
  })) document.getElementById(id).value = val;
  state.sort = "name";
  state.dir = 1;
  document.getElementById("f-sort").value = "name";
  document.getElementById("f-dir").textContent = "↑";
  render();
}

function wire() {
  document.querySelectorAll(".toolbar input, .toolbar select")
    .forEach((el) => el.addEventListener("input", render));
  document.getElementById("f-sort").addEventListener("change", (e) => {
    state.sort = e.target.value;
    render();
  });
  document.getElementById("f-dir").addEventListener("click", (e) => {
    state.dir *= -1;
    e.target.textContent = state.dir > 0 ? "↑" : "↓";
    render();
  });
  document.getElementById("f-reset").addEventListener("click", resetFilters);
  document.getElementById("modal-close").addEventListener("click", closeModal);
  document.getElementById("modal").addEventListener("click", (e) => {
    if (e.target.id === "modal") closeModal();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") closeModal();
  });
}

async function resolveDetails(cards) {
  const keyed = cards.filter((c) => c.collector_number);
  for (let i = 0; i < keyed.length; i += MIN_BATCH) {
    const chunk = keyed.slice(i, i + MIN_BATCH);
    try {
      const resp = await fetch(SCRYFALL_COLLECTION, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          identifiers: chunk.map((c) => ({ set: c.set, collector_number: c.collector_number })),
        }),
      });
      if (!resp.ok) throw new Error(`Scryfall responded ${resp.status}`);
      const result = await resp.json();
      for (const card of result.data) state.cache.set(keyOf(card.set, card.collector_number), card);
    } catch (err) {
      console.warn("Scryfall resolution failed:", err);
    }
  }
}

async function init() {
  wire();
  let cards;
  try {
    const resp = await fetch("/cards");
    if (!resp.ok) throw new Error(`GET /cards responded ${resp.status}`);
    const collection = await resp.json();
    cards = collection.cards || [];
  } catch (err) {
    document.getElementById("summary").textContent = "Failed to load collection: " + err.message;
    return;
  }
  await resolveDetails(cards);
  state.entries = cards.map((c) => ({
    card: c,
    data: state.cache.get(keyOf(c.set, c.collector_number)) || null,
  }));
  populateFilterOptions();
  render();
}

async function refresh() {
  try {
    const resp = await fetch("/cards");
    if (!resp.ok) return;
    const cards = (await resp.json()).cards || [];
    const missing = cards.filter((c) => c.collector_number &&
      !state.cache.has(keyOf(c.set, c.collector_number)));
    if (missing.length) await resolveDetails(missing);
    state.entries = cards.map((c) => ({
      card: c,
      data: state.cache.get(keyOf(c.set, c.collector_number)) || null,
    }));
    populateFilterOptions();
    render();
  } catch (err) {
    console.warn("Refresh failed:", err);
  }
}

init();
setInterval(refresh, 20000);