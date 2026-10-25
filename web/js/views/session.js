import { api } from "../api.js";
import { chip, clear, clock, conditionLabels, empty, h, toast } from "../ui.js";
import { handoff } from "./table.js";

let ticker;

export async function renderSession(root, ctx) {
  clearInterval(ticker);
  const { session } = await api.session();
  if (!session) {
    clear(root).append(
      empty(
        "No session running.",
        "Nothing is on the clock. Deal a hand and pick one.",
        h("a", { class: "btn", href: "#/" }, "Go to the table")
      )
    );
    return;
  }

  const clockEl = h("p", { class: "clock", role: "timer" }, clock(session.elapsed));
  const promptSlot = h("div", {});
  const tags = conditionLabels(ctx.state.conditions, session.conditions);

  const view = h(
    "div",
    { class: "session" },
    h("p", { class: "eyebrow" }, "On the clock"),
    h("h1", {}, session.name),
    clockEl,
    h(
      "p",
      { class: "muted" },
      session.budget
        ? `You said you had ${session.budget} minutes. This one usually wants ${session.session_label}.`
        : `This one usually wants ${session.session_label}.`
    ),
    tags.length ? h("div", { class: "chips centered" }, tags.map((t) => chip(t, "gold"))) : null,
    promptSlot,
    h(
      "div",
      { class: "row centered", style: "margin-top:1rem" },
      h(
        "button",
        {
          class: "btn ghost",
          type: "button",
          onclick: () => handoff(session.appid, session.can_launch_here),
        },
        "Launch again"
      ),
      h("button", { class: "btn ghost", type: "button", onclick: () => finish(root, ctx, session) }, "I'm done")
    )
  );

  clear(root).append(view);

  const baseline = Date.now() / 1000 - session.elapsed;
  const checkpoint = session.first_impression_at - session.started_at;

  const tick = () => {
    const elapsed = Math.floor(Date.now() / 1000 - baseline);
    clockEl.textContent = clock(elapsed);
    const due = elapsed >= checkpoint;
    clockEl.classList.toggle("past", due);
    if (due && !session.first_impression_done && !promptSlot.firstChild) {
      promptSlot.append(impressionPrompt(root, ctx, session));
    }
  };
  tick();
  ticker = setInterval(tick, 1000);
  ctx.onLeave = () => clearInterval(ticker);
  ctx.hotkeys = { x: () => finish(root, ctx, session) };
}

/// Why it did not work matters more than that it did not: "I could not hear it"
/// says nothing against the game, so the deck is told which it was.
function reasonPicker(ctx, onPick) {
  return h(
    "div",
    { class: "reasons" },
    h("p", { class: "mini-head" }, "What went wrong? One tap, and the deck learns the right lesson."),
    h(
      "div",
      { class: "verdicts" },
      ctx.state.bail_reasons.map((r) =>
        h("button", { class: "btn ghost", type: "button", onclick: () => onPick(r.id) }, r.label)
      ),
      h("button", { class: "linkish", type: "button", onclick: () => onPick(null) }, "Skip")
    )
  );
}

/// The honesty check: a few minutes in, is this actually working?
function impressionPrompt(root, ctx, session) {
  const box = h("div", { class: "impression" });
  const ask = () =>
    clear(box).append(
      h("p", { class: "eyebrow" }, "Checkpoint"),
      h("p", { style: "font-size:var(--step-1);margin:0 0 1rem" }, "Is it working?"),
      h(
        "div",
        { class: "verdicts" },
        h(
          "button",
          {
            class: "btn go",
            type: "button",
            onclick: async () => {
              await api.impression(session.id, "in");
              session.first_impression_done = true;
              toast("Good. Go away and play it.");
              renderSession(root, ctx);
            },
          },
          "Yes — leave me alone"
        ),
        h(
          "button",
          {
            class: "btn ghost danger",
            type: "button",
            onclick: () => why(),
          },
          "No — bail out"
        )
      ),
      h(
        "p",
        { class: "keyhint", style: "margin-top:1rem" },
        "Bailing is allowed. It teaches the deck more than finishing does."
      )
    );

  const why = () =>
    clear(box).append(
      h("p", { class: "eyebrow" }, "Bailing out"),
      reasonPicker(ctx, async (reason) => {
        try {
          await api.impression(session.id, "out");
          await api.endSession(session.id, "bounced", reason);
          toast("Bailed. No guilt — that is data, not failure.");
          rebound(root, ctx, session);
        } catch (e) {
          toast(e.message);
        }
      })
    );

  ask();
  return box;
}

async function rebound(root, ctx, session) {
  clear(root).append(h("p", { class: "spinner" }, "finding the next one…"));
  try {
    const card = await api.rebound({
      budget: session.budget,
      mood: session.mood,
      appetite: session.appetite,
      conditions: session.conditions,
      exclude: [session.appid],
    });
    clear(root).append(
      h(
        "div",
        { class: "session" },
        h("p", { class: "eyebrow" }, card.slot_label),
        h("h1", {}, card.name),
        h("p", { class: "muted", style: "max-width:34rem;margin:0 auto" }, card.verdict),
        card.fits && card.fits.length
          ? h(
              "div",
              { class: "chips centered" },
              card.fits.map((f) => chip(f.standing === "good" ? f.chip : f.warn, f.standing === "good" ? "gold" : "warn", f.text))
            )
          : null,
        h(
          "div",
          { class: "row centered", style: "margin-top:1.4rem" },
          h(
            "button",
            {
              class: "btn go",
              type: "button",
              onclick: async () => {
                try {
                  const res = await api.startSession({
                    appid: card.appid,
                    budget: session.budget,
                    mood: session.mood,
                    appetite: session.appetite,
                    conditions: session.conditions,
                  });
                  handoff(card.appid, res.session.can_launch_here);
                  renderSession(root, ctx);
                } catch (e) {
                  toast(e.message);
                }
              },
            },
            "Play this instead"
          ),
          h("a", { class: "btn ghost", href: "#/" }, "Back to the table")
        )
      )
    );
  } catch (e) {
    toast(e.message);
    location.hash = "#/";
  }
}

function finish(root, ctx, session) {
  clearInterval(ticker);
  const stage = h("div", { class: "session" });
  const verdicts = () =>
    clear(stage).append(
      h("p", { class: "eyebrow" }, "Verdict"),
      h("h1", {}, session.name),
      h("p", { class: "muted" }, "One word, and the deck gets smarter."),
      h(
        "div",
        { class: "verdicts" },
        verdictBtn(session, "loved", "Loved it", "go"),
        verdictBtn(session, "kept", "Kept playing", ""),
        verdictBtn(session, "fine", "It was fine", ""),
        h(
          "button",
          { class: "btn ghost", type: "button", onclick: () => notTonight() },
          "Not tonight"
        ),
        verdictBtn(session, "never", "Never again", "danger")
      ),
      h(
        "p",
        { class: "keyhint", style: "margin-top:1.4rem" },
        "\"Never again\" retires it from the deck for good."
      )
    );

  const notTonight = () =>
    clear(stage).append(
      h("p", { class: "eyebrow" }, "Not tonight"),
      h("h1", {}, session.name),
      reasonPicker(ctx, async (reason) => {
        await api.endSession(session.id, "bounced", reason);
        toast("Logged. It will come back when the night suits it.");
        location.hash = "#/reckoning";
      })
    );

  verdicts();
  clear(root).append(stage);
}

function verdictBtn(session, verdict, label, extra) {
  return h(
    "button",
    {
      class: `btn ghost ${extra}`,
      type: "button",
      onclick: async () => {
        await api.endSession(session.id, verdict);
        toast(
          verdict === "never"
            ? `${session.name} retired. That is one fewer thing to feel bad about.`
            : "Logged. Goodnight."
        );
        location.hash = "#/reckoning";
      },
    },
    label
  );
}
