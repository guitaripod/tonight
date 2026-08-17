export function h(tag, props = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props || {})) {
    if (v === null || v === undefined || v === false) continue;
    if (k === "class") el.className = v;
    else if (k === "html") el.innerHTML = v;
    else if (k.startsWith("on") && typeof v === "function") el.addEventListener(k.slice(2), v);
    else if (k === "dataset") Object.assign(el.dataset, v);
    else if (v === true) el.setAttribute(k, "");
    else el.setAttribute(k, v);
  }
  for (const c of children.flat(4)) {
    if (c === null || c === undefined || c === false) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

export function clear(node) {
  while (node.firstChild) node.firstChild.remove();
  return node;
}

export function art(appid, kind = "capsule", alt = "") {
  const img = h("img", {
    class: kind === "capsule" ? "art" : "art wide",
    src: `/img/${appid}/${kind}`,
    alt,
    loading: "lazy",
    decoding: "async",
  });
  img.addEventListener("error", () => img.classList.add("missing"), { once: true });
  return img;
}

export function thumb(appid, alt = "") {
  const img = h("img", { class: "thumb", src: `/img/${appid}/capsule`, alt, loading: "lazy" });
  img.addEventListener("error", () => { img.style.visibility = "hidden"; }, { once: true });
  return img;
}

let toastTimer;
export function toast(message, ms = 3800) {
  const el = document.getElementById("toast");
  el.textContent = message;
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.hidden = true; }, ms);
}

export function sheet(build) {
  const dlg = document.getElementById("sheet");
  const body = clear(document.getElementById("sheetBody"));
  body.append(build(() => dlg.close()));
  if (!dlg.open) dlg.showModal();
  return dlg;
}

export function hours(mins) {
  if (!mins) return "0h";
  if (mins < 60) return `${mins}m`;
  return `${(mins / 60).toFixed(mins < 600 ? 1 : 0)}h`;
}

export function ago(unix) {
  if (!unix) return "never";
  const days = (Date.now() / 1000 - unix) / 86400;
  if (days < 1) return "today";
  if (days < 2) return "yesterday";
  if (days < 14) return `${Math.round(days)}d ago`;
  if (days < 60) return `${Math.round(days / 7)}w ago`;
  if (days < 730) return `${Math.round(days / 30.4)}mo ago`;
  return `${(days / 365.25).toFixed(0)}y ago`;
}

export function plural(n, one, many) {
  return `${n} ${n === 1 ? one : many}`;
}

export function clock(seconds) {
  const s = Math.max(0, Math.floor(seconds));
  const m = Math.floor(s / 60);
  const hh = Math.floor(m / 60);
  const pad = (n) => String(n).padStart(2, "0");
  return hh > 0 ? `${hh}:${pad(m % 60)}:${pad(s % 60)}` : `${m}:${pad(s % 60)}`;
}

export function loading(text = "dealing") {
  return h("p", { class: "spinner" }, `${text}…`);
}

export function empty(title, body, action) {
  return h("div", { class: "empty" }, h("h2", {}, title), h("p", {}, body), action);
}
