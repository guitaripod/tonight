import { api } from "../api.js";
import { art, clear, h, hours, toast } from "../ui.js";

const ROUNDS = 7;

/// Straight pairwise choices. Cheap for the player, dense in information for the
/// model — seven taps and the deck knows something real about your taste.
export async function renderDuel(root, ctx) {
  let round = 1;
  const seen = [];

  const stage = h("div", {});
  clear(root).append(
    h(
      "div",
      {},
      h("p", { class: "eyebrow" }, "This or that"),
      h("h1", { class: "display", style: "font-size:var(--step-3)" }, "Don't think. Just point."),
      h(
        "p",
        { class: "lede" },
        "Seven forced choices between two games you own. No right answers — the deck is learning what you reach for."
      ),
      stage
    )
  );

  const next = async () => {
    if (round > ROUNDS) {
      clear(stage).append(
        h("p", { class: "eyebrow" }, "Done"),
        h("h2", { style: "font-weight:400;font-size:var(--step-2)" }, "The deck has your number."),
        h(
          "p",
          { class: "muted" },
          "Those seven answers now weigh on every hand you are dealt."
        ),
        h(
          "div",
          { class: "row", style: "margin-top:1.2rem" },
          h("a", { class: "btn", href: "#/" }, "Deal a hand"),
          h(
            "button",
            {
              class: "btn ghost",
              type: "button",
              onclick: () => {
                round = 1;
                next();
              },
            },
            "Seven more"
          )
        )
      );
      return;
    }

    clear(stage).append(h("p", { class: "spinner" }, "shuffling…"));
    try {
      const pair = await api.duel(seen, round, ROUNDS);
      seen.push(pair.a.appid, pair.b.appid);
      clear(stage).append(
        h(
          "div",
          {},
          h("p", { class: "keyhint", style: "margin-bottom:1rem" }, `Round ${round} of ${ROUNDS}`),
          h(
            "div",
            { class: "duel" },
            side(pair.a, () => choose(pair.a.appid, pair.b.appid)),
            h("span", { class: "versus" }, "OR"),
            side(pair.b, () => choose(pair.b.appid, pair.a.appid))
          ),
          h(
            "button",
            {
              class: "linkish",
              type: "button",
              style: "margin-top:1.4rem",
              onclick: () => {
                round += 1;
                next();
              },
            },
            "Neither — skip"
          )
        )
      );
      ctx.hotkeys = {
        arrowleft: () => choose(pair.a.appid, pair.b.appid),
        arrowright: () => choose(pair.b.appid, pair.a.appid),
      };
    } catch (e) {
      clear(stage).append(h("p", { class: "muted" }, e.message));
    }
  };

  const choose = async (winner, loser) => {
    try {
      await api.duelResult(winner, loser);
    } catch (e) {
      toast(e.message);
    }
    round += 1;
    next();
  };

  next();
}

function side(g, onPick) {
  return h(
    "button",
    { class: "duel-side", type: "button", onclick: onPick },
    art(g.appid, "header", g.name),
    h(
      "div",
      { class: "body" },
      h("h3", {}, g.name),
      h(
        "p",
        { class: "keyhint" },
        [g.session_label, g.genres.slice(0, 2).join(" · "), g.playtime_min ? `${hours(g.playtime_min)} played` : "never opened"]
          .filter(Boolean)
          .join("  ·  ")
      )
    )
  );
}
