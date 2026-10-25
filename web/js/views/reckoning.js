import { api } from "../api.js";
import { ago, clear, h, plural, thumb, toast } from "../ui.js";

function stat(label, value, sub, hot) {
  return h(
    "div",
    { class: `stat ${hot ? "hot" : ""}` },
    h("dt", {}, label),
    h("dd", {}, value, sub ? h("small", {}, ` ${sub}`) : null)
  );
}

export async function renderReckoning(root) {
  clear(root).append(h("p", { class: "spinner" }, "counting…"));
  const r = await api.reckoning();

  const money = r.money_untouched > 0
    ? `${r.money_untouched.toFixed(0)} ${r.currency}`
    : "—";

  const view = h(
    "div",
    {},
    h("p", { class: "eyebrow" }, "The reckoning"),
    h("h1", { class: "display" }, "The arithmetic of your backlog."),
    h("p", { class: "verdict-line" }, r.verdict),
    r.coverage_note ? h("p", { class: "muted", style: "max-width:56ch;font-size:0.92rem" }, r.coverage_note) : null,
    h(
      "dl",
      { class: "stat-grid" },
      stat("Games", r.owned),
      stat("Never opened", r.untouched, `${r.untouched_pct.toFixed(0)}%`, r.untouched_pct > 40),
      stat("Hours played", Math.round(r.hours_played)),
      stat("Hours left", Math.round(r.backlog_hours), "", true),
      stat("At your pace", r.years_to_clear > 400 ? "never" : `${r.years_to_clear.toFixed(0)}y`, `finish ${r.finish_year}`),
      stat("Unopened value", money, "list"),
      stat("Retired", r.amnesty_count, `${Math.round(r.amnesty_hours_freed)}h freed`),
      stat("Sessions here", r.sessions_logged, `${Math.round(r.hours_logged)}h`)
    ),
    h(
      "p",
      { class: "keyhint" },
      `Pace assumed: ${r.hours_per_week.toFixed(1)} hours a week. `,
      h("a", { href: "#/settings" }, "Change it")
    ),
    h("hr", { class: "rule" }),
    namedRow("The oldest ghost", r.oldest_ghost),
    namedRow("The one you keep refusing", r.most_passed),
    namedRow("Your longest relationship", r.biggest_love),
    readinessSection(r),
    r.top_archetypes.length
      ? h(
          "div",
          { style: "margin:2rem 0" },
          h("p", { class: "eyebrow" }, "What you actually play"),
          h(
            "div",
            { class: "axes" },
            r.top_archetypes.map(([name, hrs]) =>
              h(
                "div",
                { class: "axis" },
                h("span", {}, name),
                h(
                  "div",
                  { class: "track" },
                  h("i", {
                    style: `width:${Math.round((hrs / r.top_archetypes[0][1]) * 100)}%`,
                  })
                )
              )
            )
          )
        )
      : null,
    r.rescued.length
      ? h(
          "div",
          { style: "margin:2rem 0" },
          h("p", { class: "eyebrow" }, "Rescued from the pile"),
          h(
            "ul",
            { class: "shelf-list" },
            r.rescued.map((g) =>
              h(
                "li",
                { class: "shelf-item" },
                thumb(g.appid, g.name),
                h("div", {}, h("h3", {}, g.name), h("p", {}, g.detail)),
                h("div", {})
              )
            )
          )
        )
      : null,
    h("hr", { class: "rule" }),
    amnestySection(root)
  );

  clear(root).append(view);
}

/// What the library is good for: how many games survive each circumstance, so
/// "Sound is taken" comes with an honest count of what it leaves you.
function readinessSection(r) {
  if (!r.readiness || !r.owned) return null;
  return h(
    "div",
    { style: "margin:2rem 0" },
    h("p", { class: "eyebrow" }, "What your library is good for"),
    h(
      "p",
      { class: "muted", style: "max-width:56ch;margin-top:-0.3rem" },
      "How many of your games work when something is in the way. A small number here is the reason a bad night feels like having nothing to play."
    ),
    h(
      "div",
      { class: "axes readiness" },
      r.readiness.map((x) =>
        h(
          "div",
          { class: "axis" },
          h("span", {}, x.label),
          h(
            "div",
            { class: "axis-body" },
            h("div", { class: "track" }, h("i", { style: `width:${Math.round((x.works / Math.max(x.total, 1)) * 100)}%` })),
            h("p", { class: "axis-because" }, `${x.works} of ${x.total} games`)
          )
        )
      )
    ),
    r.bail_reasons && r.bail_reasons.length
      ? h(
          "p",
          { class: "keyhint", style: "margin-top:1rem" },
          "Why you bail: ",
          r.bail_reasons.map(([label, n]) => `${label} ×${n}`).join(" · ")
        )
      : null
  );
}

function namedRow(label, named) {
  if (!named) return null;
  return h(
    "div",
    { class: "shelf-item", style: "grid-template-columns:3.2rem 1fr auto" },
    thumb(named.appid, named.name),
    h(
      "div",
      {},
      h("h3", {}, named.name),
      h("p", {}, named.detail)
    ),
    h("p", { class: "keyhint" }, label)
  );
}

/// Amnesty: the release valve. Retiring a game is framed as arithmetic, not defeat.
function amnestySection(root) {
  const list = h("div", {});
  const section = h(
    "section",
    {},
    h("p", { class: "eyebrow" }, "Amnesty"),
    h("h2", { class: "display", style: "font-size:var(--step-3)" }, "Let some of them go."),
    h(
      "p",
      { class: "lede" },
      "A backlog is only a debt if you agree to owe it. Retire the ones you are never going to play; they leave the deck and stop counting against you."
    ),
    h(
      "button",
      {
        class: "btn",
        type: "button",
        onclick: async () => {
          clear(list).append(h("p", { class: "spinner" }, "picking the least likely…"));
          try {
            const { candidates } = await api.amnestyCandidates();
            renderCandidates(list, candidates, root);
          } catch (e) {
            toast(e.message);
          }
        },
      },
      "Show me the least likely"
    ),
    list
  );
  return section;
}

function renderCandidates(list, candidates, root) {
  if (!candidates.length) {
    clear(list).append(h("p", { class: "muted", style: "margin-top:1.4rem" }, "Nothing obvious left to retire. Your library is honest."));
    return;
  }
  clear(list).append(
    h(
      "ul",
      { class: "shelf-list", style: "margin-top:1.4rem" },
      candidates.map((c) => {
        const li = h(
          "li",
          { class: "shelf-item" },
          thumb(c.appid, c.name),
          h(
            "div",
            {},
            h("h3", {}, c.name),
            h("p", {}, c.why)
          ),
          h(
            "div",
            { class: "actions" },
            h(
              "button",
              {
                class: "btn ghost danger",
                type: "button",
                onclick: async () => {
                  await api.amnesty(c.appid, true);
                  li.remove();
                  toast(`${c.name} retired. ${Math.round(c.hours)} hours off the debt.`);
                },
              },
              "Retire"
            ),
            h(
              "button",
              {
                class: "btn ghost",
                type: "button",
                onclick: async () => {
                  await api.pin(c.appid, true);
                  li.remove();
                  toast(`${c.name} pinned instead. Prove it.`);
                },
              },
              "Keep it"
            )
          )
        );
        return li;
      })
    ),
    h(
      "button",
      {
        class: "linkish",
        type: "button",
        style: "margin-top:1rem",
        onclick: () => renderReckoning(root),
      },
      "Recount"
    )
  );
}

export async function renderHistory(root) {
  clear(root).append(h("p", { class: "spinner" }, "reading the log…"));
  const { sessions } = await api.sessions();
  clear(root).append(
    h("p", { class: "eyebrow" }, "The log"),
    h("h1", { class: "display", style: "font-size:var(--step-3)" }, "Every night you actually played."),
    sessions.length
      ? h(
          "ul",
          { class: "shelf-list" },
          sessions.map((s) =>
            h(
              "li",
              { class: "shelf-item" },
              thumb(s.appid, s.name),
              h(
                "div",
                {},
                h("h3", {}, s.name),
                h("p", {}, `${plural(s.minutes, "minute", "minutes")} · ${ago(s.started_at)}`)
              ),
              h(
                "p",
                { class: "keyhint" },
                [s.verdict, s.reason ? `(${s.reason})` : null].filter(Boolean).join(" ")
              )
            )
          )
        )
      : h("p", { class: "muted" }, "Nothing logged yet. The first session writes the first line.")
  );
}
