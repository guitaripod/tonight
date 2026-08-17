import { api } from "../api.js";
import { art, clear, h, hours, sheet, toast } from "../ui.js";

const STORE = "tonight.intake";
const HAND = "tonight.hand";

const defaults = { budget: 45, mood: "mellow", appetite: "surprise" };

function intake() {
  try {
    return { ...defaults, ...JSON.parse(localStorage.getItem(STORE) || "{}") };
  } catch {
    return { ...defaults };
  }
}

function saveIntake(patch) {
  const next = { ...intake(), ...patch };
  localStorage.setItem(STORE, JSON.stringify(next));
  return next;
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

/// The intake: three questions, asked in words, answered in one tap each.
function renderRitual(root, ctx) {
  const chosen = intake();
  const state = ctx.state;

  const question = (index, prompt, options, key, valueOf) =>
    h(
      "section",
      { class: "question" },
      h("h2", {}, h("span", { class: "index" }, index), prompt),
      h(
        "div",
        { class: "choices" },
        options.map((o) => {
          const value = valueOf(o);
          const btn = h(
            "button",
            {
              class: "choice",
              type: "button",
              "aria-pressed": String(chosen[key] === value),
              onclick: () => {
                chosen[key] = value;
                saveIntake({ [key]: value });
                btn.parentElement.querySelectorAll(".choice").forEach((b) =>
                  b.setAttribute("aria-pressed", "false")
                );
                btn.setAttribute("aria-pressed", "true");
              },
            },
            o.label,
            o.hint ? h("span", { class: "hint" }, o.hint) : null
          );
          return btn;
        })
      )
    );

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

  const games = state?.library?.games ?? 0;
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
      question(
        "How long",
        "How much of the night is actually yours?",
        state.budgets.map((b) => ({ label: b.label, value: b.minutes })),
        "budget",
        (o) => o.value
      ),
      question(
        "How you are",
        "And what state are you in?",
        state.moods.map((m) => ({ label: m.label, hint: m.hint, value: m.id })),
        "mood",
        (o) => o.value
      ),
      question(
        "What you want",
        "What are you actually hungry for?",
        state.appetites.map((a) => ({ label: a.label, hint: a.hint, value: a.id })),
        "appetite",
        (o) => o.value
      ),
      h(
        "div",
        { class: "deal-bar" },
        dealBtn,
        h(
          "span",
          { class: "keyhint" },
          "Three cards. Two passes. One game. ",
          h("kbd", {}, "D"),
          " deals."
        )
      ),
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

  root.querySelector(".btn")?.focus?.();
  ctx.hotkeys = { d: () => dealBtn.click() };
}

function chipsFor(card) {
  const out = [
    card.caution
      ? h("span", { class: "chip warn" }, card.caution)
      : h("span", { class: "chip" }, card.session_label),
  ];
  if (!card.installed) out.push(h("span", { class: "chip warn" }, "needs install"));
  if (card.playtime_min > 0) out.push(h("span", { class: "chip" }, `${hours(card.playtime_min)} played`));
  else out.push(h("span", { class: "chip gold" }, "never opened"));
  if (card.metacritic > 0) out.push(h("span", { class: "chip" }, `MC ${card.metacritic}`));
  return out;
}

/// The hand. Cards land face-down, then turn over one at a time.
export function renderHand(root, ctx, hand, { animate }) {
  const locked = hand.cards.length <= 1;
  const seats = h("div", { class: "hand", dataset: { count: String(hand.cards.length) } });

  hand.cards.forEach((card, i) => seats.append(seat(root, ctx, hand, card, i, animate, locked)));

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
          h(
            "h1",
            { class: "display", style: "font-size:var(--step-3)" },
            locked ? "This is the one." : "Three ways to spend the evening."
          )
        ),
        h(
          "p",
          { class: "context" },
          `${hand.budget ? hand.budget + " min" : "open evening"} · ${hand.mood} · ${hand.appetite}`
        )
      ),
      locked ? h("p", { class: "locked-note" }, "no more passes") : null,
      seats,
      hand.note ? h("p", { class: "reroll-note" }, hand.note) : null,
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

  ctx.hotkeys = {};
  hand.cards.forEach((card, i) => {
    ctx.hotkeys[String(i + 1)] = () => play(root, ctx, hand, card);
    if (!locked) ctx.hotkeys["qwe"[i]] = () => pass(root, ctx, hand, card, i);
  });
}

function seat(root, ctx, hand, card, index, animate, locked) {
  const face = h(
    "div",
    { class: "face face-front" },
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
        h("div", { class: "chips" }, chipsFor(card))
      )
    )
  );

  const cardEl = h(
    "div",
    { class: "card" },
    h("div", { class: "card-inner" }, h("div", { class: "face face-back" }, h("span", {}, "t")), face)
  );

  if (animate) setTimeout(() => cardEl.classList.add("revealed"), 120 + index * 190);
  else cardEl.classList.add("revealed");

  const seatEl = h(
    "div",
    { class: "seat" },
    cardEl,
    h("p", { class: "seat-blurb" }, card.slot_blurb),
    h(
      "div",
      { class: "seat-actions" },
      h(
        "button",
        { class: "btn go", type: "button", onclick: () => play(root, ctx, hand, card) },
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
    h(
      "button",
      { class: "linkish", type: "button", onclick: () => why(card) },
      "Why this?"
    )
  );
  return seatEl;
}

async function pass(root, ctx, hand, card, index) {
  const seats = root.querySelectorAll(".seat");
  const target = seats[index];
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

async function play(root, ctx, hand, card) {
  try {
    const { session } = await api.startSession({
      appid: card.appid,
      hand_id: hand.id,
      budget: hand.budget,
      mood: hand.mood,
      appetite: hand.appetite,
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

function axis(label, value) {
  return h(
    "div",
    { class: "axis" },
    h("span", {}, label),
    h("div", { class: "track" }, h("i", { style: `width:${Math.round(value * 100)}%` }))
  );
}

export async function why(card) {
  sheet((close) => {
    const body = h(
      "div",
      {},
      h("p", { class: "eyebrow" }, card.slot_label),
      h("h2", {}, card.name),
      h("p", { class: "muted" }, card.short_desc || "No description synced yet."),
      h(
        "ul",
        { class: "reason-list" },
        (card.reasons || []).map((r) => h("li", {}, r.text)),
        (card.reasons || []).length ? null : h("li", {}, "Nothing decisive — the deck simply liked it.")
      ),
      h("div", { class: "axes" }, h("p", { class: "spinner" }, "reading the game…")),
      h(
        "div",
        { class: "row", style: "margin-top:1.4rem" },
        h(
          "a",
          { class: "btn ghost", href: `https://store.steampowered.com/app/${card.appid}`, target: "_blank", rel: "noreferrer" },
          "Store page"
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
        h(
          "button",
          {
            class: "btn ghost",
            type: "button",
            onclick: async () => {
              await api.pin(card.appid, true);
              toast(`${card.name} pinned — it stays in the deck until you play it.`);
              close();
            },
          },
          "Pin it"
        ),
        h("button", { class: "btn ghost", type: "button", onclick: close }, "Close")
      )
    );

    api.game(card.appid).then((g) => {
      const box = body.querySelector(".axes");
      clear(box).append(
        axis("Pace", g.shape.pace),
        axis("Thinking", g.shape.brain),
        axis("Story", g.shape.story),
        axis("Company", g.shape.social),
        axis("Calm", g.shape.chill),
        h(
          "p",
          { class: "keyhint", style: "margin-top:0.8rem" },
          `${g.shape.archetype} · typical sitting ${g.shape.session_minutes} min · about ${Math.round(
            g.shape.completion_hours
          )}h to see it through`
        )
      );
    }).catch(() => {});

    return body;
  });
}
