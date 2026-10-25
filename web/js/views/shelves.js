import { api } from "../api.js";
import { clear, h, thumb, toast } from "../ui.js";
import { showGame } from "./game.js";

let shelfDefs = null;

function shelfNav() {
  const groups = [...new Set(shelfDefs.map((s) => s.group))];
  return h(
    "div",
    { class: "shelf-groups" },
    groups.map((group) =>
      h(
        "section",
        { class: "shelf-group" },
        h("p", { class: "mini-head" }, group),
        h(
          "div",
          { class: "shelf-nav" },
          shelfDefs
            .filter((s) => s.group === group)
            .map((s) =>
              h(
                "button",
                {
                  class: "linkish",
                  type: "button",
                  onclick: () => {
                    location.hash = `#/shelves/${s.id}`;
                  },
                },
                s.title,
                h("span", { class: "q" }, s.question)
              )
            )
        )
      )
    )
  );
}

export async function renderShelves(root, ctx, shelfId) {
  if (!shelfDefs) shelfDefs = (await api.shelves()).shelves;

  const results = h("div", { id: "results" });
  const input = h("input", {
    class: "search",
    type: "search",
    placeholder: "Type a name, a genre, a mood…",
    autocomplete: "off",
    spellcheck: "false",
    "aria-label": "Search your library",
  });

  let timer;
  input.addEventListener("input", () => {
    clearTimeout(timer);
    const q = input.value.trim();
    if (q.length < 2) {
      clear(results);
      return;
    }
    timer = setTimeout(async () => {
      try {
        const { items } = await api.search(q);
        renderList(results, items, `${items.length} match${items.length === 1 ? "" : "es"} for “${q}”`);
      } catch (e) {
        toast(e.message);
      }
    }, 180);
  });

  clear(root).append(
    h(
      "div",
      {},
      h("p", { class: "eyebrow" }, "The vault"),
      h("h1", { class: "display", style: "font-size:var(--step-3)" }, "No wall of boxes. Ask a question instead."),
      input,
      shelfNav(),
      results
    )
  );

  ctx.hotkeys = { "/": () => input.focus() };
  if (shelfId) {
    try {
      const shelf = await api.shelf(shelfId, 80);
      renderList(results, shelf.items, `${shelf.title} — ${shelf.count}`, shelf.question);
      results.scrollIntoView({ behavior: "smooth", block: "start" });
    } catch (e) {
      toast(e.message);
    }
  } else {
    input.focus();
  }
}

function renderList(container, items, heading, sub) {
  clear(container);
  container.append(
    h("p", { class: "eyebrow", style: "margin-top:1.6rem" }, heading),
    sub ? h("p", { class: "muted", style: "margin-top:-0.3rem" }, sub) : null
  );
  if (!items.length) {
    container.append(h("p", { class: "muted" }, "Nothing on this shelf. That is a good sign."));
    return;
  }
  container.append(
    h(
      "ul",
      { class: "shelf-list" },
      items.map((it) => {
        const li = h(
          "li",
          { class: "shelf-item" },
          thumb(it.appid, it.name),
          h(
            "div",
            {},
            h("h3", {}, it.name),
            h(
              "p",
              {},
              [it.detail, it.session_label, it.installed ? "installed" : null]
                .filter(Boolean)
                .join("  ·  ")
            )
          ),
          h(
            "div",
            { class: "actions" },
            h(
              "button",
              {
                class: "btn ghost",
                type: "button",
                onclick: () =>
                  showGame({
                    appid: it.appid,
                    name: it.name,
                    slot_label: it.amnesty ? "Retired" : "From the vault",
                    short_desc: it.short_desc,
                    tags: it.tags,
                    reasons: [{ text: it.detail }],
                  }),
              },
              "Look"
            ),
            it.amnesty
              ? h(
                  "button",
                  {
                    class: "btn ghost",
                    type: "button",
                    onclick: async () => {
                      await api.amnesty(it.appid, false);
                      li.remove();
                      toast(`${it.name} is back in the deck.`);
                    },
                  },
                  "Un-retire"
                )
              : h(
                  "a",
                  { class: "btn go", href: `steam://rungameid/${it.appid}` },
                  "Play"
                )
          )
        );
        return li;
      })
    )
  );
}
