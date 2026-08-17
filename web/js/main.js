import { api } from "./api.js";
import { clear, h, toast } from "./ui.js";
import { renderTable } from "./views/table.js";
import { renderSession } from "./views/session.js";
import { renderReckoning, renderHistory } from "./views/reckoning.js";
import { renderShelves } from "./views/shelves.js";
import { renderDuel } from "./views/duel.js";
import { renderSettings } from "./views/settings.js";

const root = document.getElementById("app");
const navEl = document.getElementById("nav");
const pill = document.getElementById("syncPill");

const ctx = { state: null, hotkeys: {}, onLeave: null };

const NAV = [
  ["#/", "Table"],
  ["#/shelves", "Vault"],
  ["#/duel", "This or that"],
  ["#/reckoning", "Reckoning"],
  ["#/settings", "Settings"],
];

function paintNav(hash) {
  const active = (href) => (href === "#/" ? hash === "#/" || hash === "#/session" : hash.startsWith(href));
  clear(navEl).append(
    ...NAV.map(([href, label]) => h("a", { href, "aria-current": active(href) ? "page" : null }, label))
  );
}

const routes = [
  [/^#\/$/, async () => {
    const { session } = await api.session();
    if (session) return renderSession(root, ctx);
    return renderTable(root, ctx);
  }],
  [/^#\/session$/, () => renderSession(root, ctx)],
  [/^#\/shelves\/(.+)$/, (m) => renderShelves(root, ctx, m[1])],
  [/^#\/shelves$/, () => renderShelves(root, ctx, null)],
  [/^#\/duel$/, () => renderDuel(root, ctx)],
  [/^#\/reckoning$/, () => renderReckoning(root, ctx)],
  [/^#\/history$/, () => renderHistory(root, ctx)],
  [/^#\/settings$/, () => renderSettings(root, ctx)],
];

async function route() {
  const hash = location.hash || "#/";
  if (ctx.onLeave) {
    ctx.onLeave();
    ctx.onLeave = null;
  }
  ctx.hotkeys = {};
  paintNav(hash);
  const hit = routes.find(([re]) => re.test(hash));
  if (!hit) {
    location.hash = "#/";
    return;
  }
  try {
    await hit[1](hash.match(hit[0]));
  } catch (e) {
    clear(root).append(
      h("p", { class: "eyebrow" }, "That didn't work"),
      h("p", { class: "verdict-line" }, e.message),
      h("a", { class: "btn ghost", href: "#/" }, "Back to the table")
    );
  }
  root.focus({ preventScroll: true });
}

function watchSync() {
  const source = new EventSource("/api/sync/stream");
  source.onmessage = (ev) => {
    let s;
    try {
      s = JSON.parse(ev.data);
    } catch {
      return;
    }
    const active = s.running;
    pill.hidden = !active && !s.error;
    pill.classList.toggle("busy", active);
    if (s.error) {
      pill.textContent = s.error;
    } else if (active) {
      const progress = s.total > 0 ? ` ${s.done}/${s.total}` : "";
      pill.textContent = `${s.message}${progress}`;
    }
    if (!active && s.finished_at && ctx.lastSyncSeen !== s.finished_at) {
      ctx.lastSyncSeen = s.finished_at;
      if (!s.error) {
        toast("Library synced.");
        api.state().then((st) => {
          ctx.state = st;
        });
      }
      setTimeout(() => {
        if (!pill.classList.contains("busy")) pill.hidden = true;
      }, 6000);
    }
  };
  source.onerror = () => {};
  pill.addEventListener("click", () => api.cancelSync().then(() => toast("Sync stopped.")));
}

document.addEventListener("keydown", (e) => {
  if (e.metaKey || e.ctrlKey || e.altKey) return;
  const tag = document.activeElement?.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || document.getElementById("sheet").open) return;
  const key = e.key.toLowerCase();
  if (key === "?") {
    help();
    return;
  }
  const fn = ctx.hotkeys[key];
  if (fn) {
    e.preventDefault();
    fn();
  }
});

function help() {
  toast("1/2/3 play a card · Q/W/E pass · D deal · / search · X end session", 6000);
}

async function boot() {
  try {
    ctx.state = await api.state();
  } catch (e) {
    clear(root).append(h("p", { class: "verdict-line" }, `Cannot reach the server: ${e.message}`));
    return;
  }
  watchSync();
  window.addEventListener("hashchange", route);
  await route();

  const s = ctx.state;
  if (s.library.games === 0) {
    location.hash = "#/settings";
    toast("Nothing synced yet. Hit “Sync now”.");
  } else if (!s.last_sync) {
    api.sync(false).catch(() => {});
  }
}

boot();
