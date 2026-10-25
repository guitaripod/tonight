import { api } from "../api.js";
import { chip, clear, h, sheet, toast } from "../ui.js";

const MARKS = { good: "✓", okay: "~", poor: "✕" };

const FEEL = [
  ["pace", "Pace", "slow", "frantic"],
  ["brain", "Thinking", "mindless", "demanding"],
  ["story", "Story", "none", "central"],
  ["social", "Company", "solo", "multiplayer"],
  ["chill", "Calm", "tense", "relaxing"],
];

const DEMANDS = [
  ["sound", "Needs sound", "irrelevant", "essential"],
  ["pause", "Survives interruption", "never", "always"],
  ["scare", "Frightening", "safe", "scary"],
  ["online", "Needs internet", "offline", "online only"],
  ["controller", "Controller friendly", "keyboard", "gamepad"],
  ["couch", "Shared screen", "solo", "made for two"],
];

const SOUND_CHOICES = [
  [null, "Let tonight guess"],
  [0.1, "Doesn't matter"],
  [0.5, "Helps"],
  [0.9, "Essential"],
];

const SITTING_CHOICES = [
  [null, "Let tonight guess"],
  [20, "20 min"],
  [45, "45 min"],
  [90, "90 min"],
  [180, "A whole night"],
];

function meter(label, value, low, high, because) {
  return h(
    "div",
    { class: "axis" },
    h("span", {}, label),
    h(
      "div",
      { class: "axis-body" },
      h("div", { class: "track" }, h("i", { style: `width:${Math.round(value * 100)}%` })),
      h("p", { class: "axis-ends" }, h("span", {}, low), h("span", {}, high)),
      because && because.length ? h("p", { class: "axis-because" }, because.join(" · ")) : null
    )
  );
}

function segmented(label, choices, current, onPick) {
  const group = h("div", { class: "segmented", role: "group", "aria-label": label });
  const paint = (value) => {
    group.querySelectorAll("button").forEach((b) => {
      b.setAttribute("aria-pressed", String(b.dataset.value === String(value)));
    });
  };
  choices.forEach(([value, text]) => {
    group.append(
      h(
        "button",
        {
          type: "button",
          dataset: { value: String(value) },
          onclick: async () => {
            try {
              await onPick(value);
              paint(value);
            } catch (e) {
              toast(e.message);
            }
          },
        },
        text
      )
    );
  });
  paint(current);
  return h("div", { class: "field-inline" }, h("p", { class: "mini-head" }, label), group);
}

function nearest(choices, value) {
  if (value === null || value === undefined) return null;
  return choices.reduce(
    (best, [v]) => (v !== null && (best === null || Math.abs(v - value) < Math.abs(best - value)) ? v : best),
    null
  );
}

function reasonList(card) {
  const shownBelow = (card.fits || []).length > 0;
  const texts = (card.reasons || []).filter((r) => !(shownBelow && r.kind.startsWith("fit:"))).map((r) => r.text);
  if (!texts.length && card.detail) texts.push(card.detail);
  return h(
    "ul",
    { class: "reason-list" },
    texts.length ? texts.map((t) => h("li", {}, t)) : h("li", {}, "Nothing decisive. The deck simply liked it.")
  );
}

function fitRows(fits, activeIds) {
  return h(
    "ul",
    { class: "fit-list" },
    fits.map((f) =>
      h(
        "li",
        { class: `${f.standing} ${activeIds.has(f.id) ? "active" : ""}`.trim() },
        h("span", { class: "mark", "aria-hidden": "true" }, MARKS[f.standing]),
        h("div", {}, h("b", {}, f.chip), h("p", {}, f.text))
      )
    )
  );
}

function scoreBars(card) {
  const parts = (card.parts || []).filter((p) => Math.abs(p.value) > 0.005);
  if (!parts.length) return null;
  const top = Math.max(...parts.map((p) => Math.abs(p.value)), 0.01);
  return h(
    "div",
    { class: "score-card" },
    h("p", { class: "mini-head" }, "How the score adds up"),
    parts.map((p) =>
      h(
        "div",
        { class: `score-row ${p.value < 0 ? "neg" : ""}`.trim() },
        h("span", {}, p.label),
        h("div", { class: "track" }, h("i", { style: `width:${Math.round((Math.abs(p.value) / top) * 100)}%` })),
        h("b", { class: "num" }, `${p.value > 0 ? "+" : "−"}${Math.abs(p.value).toFixed(2)}`)
      )
    ),
    card.gate && card.gate < 0.995
      ? h(
          "p",
          { class: "score-gate" },
          `Tonight's conditions scale all of that by ×${card.gate.toFixed(2)}. A game that cannot work tonight is not rescued by how much you would like it.`
        )
      : null
  );
}

/// The full argument for one game: why it was dealt, how it stands against each
/// condition, how its score adds up, and the controls to overrule the guess.
export function showGame(card) {
  sheet((close) => {
    const active = new Set((card.fits || []).map((f) => f.id));
    const slot = h("div", { class: "game-detail" }, h("p", { class: "spinner" }, "reading the game…"));

    const head = h(
      "div",
      {},
      h("p", { class: "eyebrow" }, card.slot_label || "From the vault"),
      h("h2", {}, card.name),
      h("p", { class: "muted" }, card.short_desc || "No description synced yet."),
      card.tags && card.tags.length
        ? h("div", { class: "chips", style: "margin:0.9rem 0 0" }, card.tags.map((t) => chip(t)))
        : null,
      reasonList(card),
      scoreBars(card),
      slot
    );

    const actions = h(
      "div",
      { class: "row", style: "margin-top:1.6rem" },
      h(
        "a",
        { class: "btn ghost", href: `https://store.steampowered.com/app/${card.appid}`, target: "_blank", rel: "noreferrer" },
        "Store page"
      ),
      h(
        "button",
        {
          class: "btn ghost",
          type: "button",
          onclick: async () => {
            await api.pin(card.appid, true);
            toast(`${card.name} pinned. It stays in the deck until you play it.`);
            close();
          },
        },
        "Pin it"
      ),
      h(
        "button",
        {
          class: "btn ghost danger",
          type: "button",
          onclick: async () => {
            await api.amnesty(card.appid, true);
            toast(`${card.name} retired. It will not be offered again.`);
            close();
          },
        },
        "Retire it"
      ),
      h("button", { class: "btn ghost", type: "button", onclick: close }, "Close")
    );

    api
      .game(card.appid)
      .then((g) => {
        const shown = g.fits.filter((f) => (active.size ? true : f.id !== "installed"));
        clear(slot).append(
          h("p", { class: "mini-head" }, active.size ? "Against tonight's conditions" : "Against every condition"),
          fitRows(shown, active),
          h("p", { class: "mini-head" }, "What it feels like"),
          h(
            "div",
            { class: "axes" },
            FEEL.map(([key, label, low, high]) => meter(label, g.shape[key], low, high)),
            DEMANDS.map(([key, label, low, high]) =>
              meter(label, g.shape[key].value, low, high, g.shape[key].confidence > 0.3 ? g.shape[key].because : ["not enough to go on"])
            )
          ),
          h(
            "p",
            { class: "keyhint", style: "margin:0.6rem 0 1.2rem" },
            `${g.shape.archetype} · typical sitting ${g.shape.session_minutes} min · about ${Math.round(g.shape.completion_hours)}h to see it through`
          ),
          h("p", { class: "mini-head" }, "Overrule the guess"),
          segmented(
            "How much does the sound matter?",
            SOUND_CHOICES,
            nearest(SOUND_CHOICES, g.overrides.sound),
            async (value) => {
              await api.setTrait(card.appid, "sound", value);
              toast(value === null ? "Back to guessing." : "Noted. It counts from the next deal.");
            }
          ),
          segmented(
            "How long is one sitting?",
            SITTING_CHOICES,
            g.session_override ?? null,
            async (value) => {
              await api.setSessionLength(card.appid, value);
              toast(value === null ? "Back to guessing." : "Noted. It counts from the next deal.");
            }
          )
        );
      })
      .catch((e) => clear(slot).append(h("p", { class: "muted" }, e.message)));

    return h("div", {}, head, actions);
  });
}
