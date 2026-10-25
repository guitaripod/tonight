import { api } from "../api.js";
import { art, budgetLabel, chip, clear, conditionLabels, h, hours, labelOf, toast } from "../ui.js";
import { showGame } from "./game.js";

const STORE = "tonight.intake";
const HAND = "tonight.hand";

const defaults = { budget: 45, mood: "mellow", appetite: "surprise", conditions: [] };

const MARKS = { good: "✓", okay: "~", poor: "✕" };

function intake() {
  let saved = {};
  try {
    saved = JSON.parse(localStorage.getItem(STORE) || "{}");
  } catch {
    saved = {};
  }
  const merged = { ...defaults, ...saved };
  merged.conditions = Array.isArray(merged.conditions) ? merged.conditions : [];
  if (merged.mood === "social") {
    merged.mood = defaults.mood;
    merged.conditions = [...new Set([...merged.conditions, "friends"])];
  }
  return merged;
}

function saveIntake(chosen) {
  try {
    localStorage.setItem(STORE, JSON.stringify(chosen));
  } catch {
    return;
  }
}

export async function renderTable(root, ctx) {
  const held = sessionStorage.getItem(HAND);
  if (held) {
    try {
      const hand = await api.hand(held);
      if (hand && hand.cards.length) return renderHand(root, ctx, hand, { animate: false });
    } catch {
      sessionStorage.removeItem(HAND);
    }
  }
  return renderRitual(root, ctx);
}

/// The intake: three questions in words, then the things that are simply true
/// about tonight. Every tap re-counts how many games survive.
function renderRitual(root, ctx) {
  const chosen = intake();
  const state = ctx.state;
  const valid = new Set(state.conditions.map((c) => c.id));
  chosen.conditions = chosen.conditions.filter((id) => valid.has(id));

  const meter = h("p", { class: "fit-meter", "aria-live": "polite" });
  const notice = h("p", { class: "notice", hidden: true });
  let ticket = 0;
  let timer;

  const dealBtn = h(
    "button",
    {
      class: "btn",
      type: "button",
      onclick: async () => {
        dealBtn.disabled = true;
        dealBtn.textContent = "Dealing…";
        try {
          const hand = await api.deal({ ...chosen });
          sessionStorage.setItem(HAND, hand.id);
          renderHand(root, ctx, hand, { animate: true });
        } catch (e) {
          toast(e.message);
          dealBtn.disabled = false;
          dealBtn.textContent = "Deal three cards";
        }
      },
    },
    "Deal three cards"
  );

  const paint = (fit) => {
    clear(meter);
    notice.hidden = true;
    if (!fit) return;
    if (fit.viable === 0) {
      meter.append(h("b", {}, "Nothing fits tonight."));
      notice.textContent = fit.blame || "Loosen one of the conditions.";
      notice.hidden = false;
      dealBtn.disabled = true;
      dealBtn.textContent = "Nothing to deal";
      return;
    }
    dealBtn.disabled = false;
    dealBtn.textContent = fit.viable < 3 ? `Deal what is left (${fit.viable})` : "Deal three cards";
    meter.append(
      h("b", { class: "num" }, fit.viable),
      ` of ${fit.library} games fit tonight`
    );
    const text = fit.relaxed;
    if (text) {
      notice.textContent = text;
      notice.hidden = false;
    }
  };

  const refresh = () => {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const mine = ++ticket;
      try {
        const fit = await api.fit({ ...chosen });
        if (mine === ticket) paint(fit);
      } catch {
        if (mine === ticket) paint(null);
      }
    }, 120);
  };

  const pick = (key, value, group) => {
    chosen[key] = value;
    saveIntake(chosen);
    group.querySelectorAll(".choice").forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.value === String(value))));
    refresh();
  };

  const question = (index, prompt, options, key) => {
    const group = h("div", { class: "choices", role: "group", "aria-label": prompt });
    options.forEach((o) => {
      group.append(
        h(
          "button",
          {
            class: "choice",
            type: "button",
            dataset: { value: String(o.value) },
            "aria-pressed": String(chosen[key] === o.value),
            onclick: () => pick(key, o.value, group),
          },
          o.label,
          o.hint ? h("span", { class: "hint" }, o.hint) : null
        )
      );
    });
    return h("section", { class: "question" }, h("h2", {}, h("span", { class: "index" }, index), prompt), group);
  };

  const toggles = h("div", { class: "toggles", role: "group", "aria-label": "Conditions tonight" });
  state.conditions.forEach((c) => {
    const btn = h(
      "button",
      {
        class: "toggle",
        type: "button",
        dataset: { id: c.id },
        "aria-pressed": String(chosen.conditions.includes(c.id)),
        onclick: () => {
          const on = !chosen.conditions.includes(c.id);
          chosen.conditions = on ? [...chosen.conditions, c.id] : chosen.conditions.filter((x) => x !== c.id);
          btn.setAttribute("aria-pressed", String(on));
          saveIntake(chosen);
          refresh();
        },
      },
      h("span", { class: "box", "aria-hidden": "true" }),
      h("span", { class: "toggle-text" }, c.label, h("span", { class: "hint" }, c.hint))
    );
    toggles.append(btn);
  });

  const games = state?.library?.playable ?? state?.library?.games ?? 0;
  clear(root).append(
    h(
      "div",
      { class: "ritual" },
      h("p", { class: "eyebrow" }, "The table"),
      h("h1", { class: "display" }, "What are you playing tonight?"),
      h(
        "p",
        { class: "lede" },
        games
          ? `You own ${games} games. You are about to be shown three. Two of them are going away.`
          : "Nothing in the library yet — sync your Steam client first."
      ),
      question("How long", "How much of the night is actually yours?", state.budgets.map((b) => ({ label: b.label, value: b.minutes })), "budget"),
      question("How you are", "And what state are you in?", state.moods.map((m) => ({ label: m.label, hint: m.hint, value: m.id })), "mood"),
      question("What you want", "What are you actually hungry for?", state.appetites.map((a) => ({ label: a.label, hint: a.hint, value: a.id })), "appetite"),
      h(
        "section",
        { class: "question conditions" },
        h("h2", {}, h("span", { class: "index" }, "In the way"), "Is anything in the way?"),
        h("p", { class: "sub" }, "Optional. Tick what is true tonight; these rule games out before taste gets a vote."),
        toggles
      ),
      h("div", { class: "deal-bar sticky" }, dealBtn, meter, h("span", { class: "keyhint" }, "Press ", h("kbd", {}, "D"), " to deal")),
      notice,
      games
        ? h(
            "p",
            { class: "keyhint", style: "margin-top:2rem" },
            "Can't even face the question? ",
            h("a", { href: "#/duel" }, "Play seven rounds of this-or-that"),
            " and let the deck learn you."
          )
        : null
    )
  );

  dealBtn.focus({ preventScroll: true });
  ctx.hotkeys = { d: () => dealBtn.click() };
  ctx.onLeave = () => clearTimeout(timer);
  refresh();
}

function chipsFor(card) {
  const out = [
    card.caution ? chip(card.caution, "warn") : chip(card.session_label),
  ];
  if (!card.installed) out.push(chip("needs install", "warn"));
  if (card.playtime_min > 0) out.push(chip(`${hours(card.playtime_min)} played`));
  else out.push(chip("never opened", "gold"));
  if (card.metacritic > 0) out.push(chip(`MC ${card.metacritic}`));
  else if (card.review_pct >= 70) out.push(chip(`${card.review_pct}% liked`));
  return out;
}

function fitRow(card) {
  const fits = card.fits || [];
  if (!fits.length) return null;
  return h(
    "ul",
    { class: "seat-fits", "aria-label": "How it meets tonight's conditions" },
    fits.map((f) =>
      h(
        "li",
        { class: f.standing, title: f.text },
        h("span", { class: "mark", "aria-hidden": "true" }, MARKS[f.standing]),
        f.standing === "good" ? f.chip : f.warn
      )
    )
  );
}

function tonightLine(ctx, hand) {
  const s = ctx.state;
  return [
    budgetLabel(s.budgets, hand.budget),
    labelOf(s.moods, hand.mood),
    labelOf(s.appetites, hand.appetite),
    ...conditionLabels(s.conditions, hand.conditions),
  ].map((text) => h("span", {}, text));
}

/// The hand. Cards land face-down, then turn over one at a time.
export function renderHand(root, ctx, hand, { animate }) {
  const locked = hand.cards.length <= 1;
  const seats = h("div", { class: "hand", dataset: { count: String(hand.cards.length) } });

  hand.cards.forEach((card, i) => seats.append(seat(root, ctx, hand, card, i, animate, locked)));

  const heading = locked
    ? "This is the one."
    : hand.cards.length === 2
      ? "Two ways to spend the evening."
      : "Three ways to spend the evening.";

  clear(root).append(
    h(
      "div",
      {},
      h(
        "div",
        { class: "table-head" },
        h(
          "div",
          {},
          h("p", { class: "eyebrow" }, locked ? "The verdict" : "Your hand"),
          h("h1", { class: "display", style: "font-size:var(--step-3)" }, heading)
        ),
        h("p", { class: "context" }, tonightLine(ctx, hand))
      ),
      hand.relaxed ? h("p", { class: "notice" }, hand.relaxed) : null,
      locked ? h("p", { class: "locked-note" }, "no more passes") : null,
      seats,
      hand.note ? h("p", { class: "reroll-note" }, hand.note) : null,
      hand.library
        ? h("p", { class: "pool-line" }, `Dealt from ${hand.viable} of your ${hand.library} games that fit tonight.`)
        : null,
      h(
        "div",
        { class: "deal-bar" },
        h(
          "button",
          {
            class: "btn ghost",
            type: "button",
            onclick: async () => {
              try {
                const next = await api.deal({
                  budget: hand.budget,
                  mood: hand.mood,
                  appetite: hand.appetite,
                  conditions: hand.conditions,
                  reroll_of: hand.id,
                  exclude: hand.cards.map((c) => c.appid),
                });
                sessionStorage.setItem(HAND, next.id);
                renderHand(root, ctx, next, { animate: true });
              } catch (e) {
                toast(e.message);
              }
            },
          },
          "Deal again"
        ),
        h(
          "button",
          {
            class: "btn ghost",
            type: "button",
            onclick: () => {
              sessionStorage.removeItem(HAND);
              renderRitual(root, ctx);
            },
          },
          "Change the question"
        ),
        h(
          "span",
          { class: "keyhint" },
          h("kbd", {}, "1"),
          h("kbd", {}, "2"),
          h("kbd", {}, "3"),
          " play · ",
          h("kbd", {}, "Q"),
          h("kbd", {}, "W"),
          h("kbd", {}, "E"),
          " pass"
        )
      )
    )
  );

  ctx.hotkeys = { d: () => root.querySelector(".deal-bar .btn")?.click() };
  hand.cards.forEach((card, i) => {
    ctx.hotkeys[String(i + 1)] = () => play(hand, card);
    if (!locked) ctx.hotkeys["qwe"[i]] = () => pass(root, ctx, hand, card, i);
  });
}

function otherReason(card) {
  const reason = (card.reasons || []).find((r) => r.text !== card.verdict && !r.kind.startsWith("fit:"));
  return reason ? reason.text : null;
}

function seat(root, ctx, hand, card, index, animate, locked) {
  const extra = otherReason(card);
  const face = h(
    "div",
    { class: "face face-front", dataset: { initial: card.name.trim().charAt(0) } },
    art(card.appid, "capsule", card.name),
    h(
      "div",
      { class: "card-text" },
      h("p", { class: "arcana" }, card.slot_label),
      h(
        "div",
        {},
        h("h2", { class: "card-title" }, card.name),
        h("p", { class: "card-verdict" }, card.verdict),
        extra ? h("p", { class: "card-also" }, extra) : null,
        h("div", { class: "chips" }, chipsFor(card))
      )
    )
  );

  face.addEventListener("art-banner", () => {
    face.classList.add("banner-art");
    face.style.setProperty("--backdrop", `url(/img/${card.appid}/header)`);
  });

  const cardEl = h(
    "div",
    { class: "card" },
    h("div", { class: "card-inner" }, h("div", { class: "face face-back" }, h("span", {}, "t")), face)
  );

  if (animate) setTimeout(() => cardEl.classList.add("revealed"), 120 + index * 190);
  else cardEl.classList.add("revealed");

  return h(
    "div",
    { class: "seat" },
    cardEl,
    fitRow(card),
    h("p", { class: "seat-blurb" }, card.slot_blurb),
    h(
      "div",
      { class: "seat-actions" },
      h(
        "button",
        { class: "btn go", type: "button", onclick: () => play(hand, card) },
        locked ? "Play it" : `Play (${index + 1})`
      ),
      locked
        ? null
        : h(
            "button",
            {
              class: "btn ghost",
              type: "button",
              onclick: () => pass(root, ctx, hand, card, index),
            },
            "Pass"
          )
    ),
    h("button", { class: "linkish", type: "button", onclick: () => showGame(card) }, "Why this?")
  );
}

async function pass(root, ctx, hand, card, index) {
  const target = root.querySelectorAll(".seat")[index];
  if (target) target.classList.add("leaving");
  try {
    const next = await new Promise((resolve, reject) =>
      setTimeout(() => api.pass(hand.id, card.appid).then(resolve, reject), 260)
    );
    renderHand(root, ctx, next, { animate: false });
    if (next.cards.length === 1) toast("That's the one. Go and play it.");
  } catch (e) {
    toast(e.message);
    if (target) target.classList.remove("leaving");
  }
}

async function play(hand, card) {
  try {
    const { session } = await api.startSession({
      appid: card.appid,
      hand_id: hand.id,
      budget: hand.budget,
      mood: hand.mood,
      appetite: hand.appetite,
      conditions: hand.conditions,
    });
    sessionStorage.removeItem(HAND);
    handoff(card.appid, session.can_launch_here);
    location.hash = "#/session";
  } catch (e) {
    toast(e.message);
  }
}

/// Hands the game to the OS: a server-side launch if enabled, the steam:// URL otherwise.
export async function handoff(appid, canLaunchHere) {
  if (canLaunchHere) {
    try {
      const res = await api.launch(appid);
      toast(`Launching on the host — ${res.detail}`);
      return;
    } catch (e) {
      toast(`${e.message} Falling back to the steam:// link.`);
    }
  }
  window.location.href = `steam://rungameid/${appid}`;
}
