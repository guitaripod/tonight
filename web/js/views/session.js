import { api } from "../api.js";
import { clear, clock, empty, h, toast } from "../ui.js";
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

  const clockEl = h("p", { class: "clock" }, clock(session.elapsed));
  const promptSlot = h("div", {});

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
    promptSlot,
    h(
      "div",
      { class: "row", style: "justify-content:center;margin-top:1rem" },
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

/// The honesty check: a few minutes in, is this actually working?
function impressionPrompt(root, ctx, session) {
  return h(
    "div",
    { class: "impression" },
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
          onclick: async () => {
            await api.impression(session.id, "out");
            await api.endSession(session.id, "bounced");
            toast("Bailed. No guilt — that is data, not failure.");
            rebound(root, ctx, session);
          },
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
}

async function rebound(root, ctx, session) {
  clear(root).append(h("p", { class: "spinner" }, "finding the next one…"));
  try {
    const card = await api.rebound({
      budget: session.budget,
      mood: session.mood,
      appetite: session.appetite,
      exclude: [session.appid],
    });
    clear(root).append(
      h(
        "div",
        { class: "session" },
        h("p", { class: "eyebrow" }, card.slot_label),
        h("h1", {}, card.name),
        h("p", { class: "muted", style: "max-width:34rem;margin:0 auto" }, card.verdict),
        h(
          "div",
          { class: "row", style: "justify-content:center;margin-top:1.4rem" },
          h(
            "button",
            {
              class: "btn go",
              type: "button",
              onclick: async () => {
                const res = await api.startSession({
                  appid: card.appid,
                  budget: session.budget,
                  mood: session.mood,
                  appetite: session.appetite,
                });
                handoff(card.appid, res.session.can_launch_here);
                renderSession(root, ctx);
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
  clear(root).append(
    h(
      "div",
      { class: "session" },
      h("p", { class: "eyebrow" }, "Verdict"),
      h("h1", {}, session.name),
      h("p", { class: "muted" }, "One word, and the deck gets smarter."),
      h(
        "div",
        { class: "verdicts" },
        verdictBtn(session, "loved", "Loved it", "go"),
        verdictBtn(session, "kept", "Kept playing", ""),
        verdictBtn(session, "fine", "It was fine", ""),
        verdictBtn(session, "bounced", "Not tonight", ""),
        verdictBtn(session, "never", "Never again", "danger")
      ),
      h(
        "p",
        { class: "keyhint", style: "margin-top:1.4rem" },
        "\"Never again\" retires it from the deck for good."
      )
    )
  );
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
