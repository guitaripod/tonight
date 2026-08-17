import { api } from "../api.js";
import { ago, clear, h, toast } from "../ui.js";

export async function renderSettings(root, ctx) {
  const s = await api.state();
  ctx.state = s;

  const field = (label, input, help) =>
    h("div", { class: "field" }, h("label", { for: input.id }, label), input, help ? h("p", { class: "help" }, help) : null);

  const check = (id, label, checked, help) => {
    const input = h("input", { type: "checkbox", id, checked: checked || false });
    return h(
      "div",
      { class: "field check" },
      input,
      h("div", {}, h("label", { for: id }, label), help ? h("p", { class: "help" }, help) : null)
    );
  };

  const key = h("input", {
    type: "password",
    id: "key",
    placeholder: s.settings.has_api_key ? "•••••••• (saved)" : "optional",
    autocomplete: "off",
  });
  const steamid = h("input", { type: "text", id: "steamid", value: s.settings.steamid || "" });
  const impression = h("input", {
    type: "number",
    id: "impression",
    min: "1",
    max: "120",
    value: s.settings.first_impression_minutes,
  });
  const pace = h("input", {
    type: "number",
    id: "pace",
    min: "0",
    step: "0.5",
    value: s.settings.hours_per_week || "",
    placeholder: "auto",
  });
  const cc = h("input", { type: "text", id: "cc", value: s.settings.store_cc, maxlength: "2" });
  const launchBox = check(
    "launch",
    "Let this server start games on its own machine",
    s.settings.allow_launch,
    s.launch_possible
      ? "With this on, picking a game from your phone starts it on the PC running tonight. Off by default — it runs a command on the host."
      : "No steam, flatpak or xdg-open found on this host, so nothing would happen."
  );

  const save = h(
    "button",
    {
      class: "btn",
      type: "button",
      onclick: async () => {
        try {
          await api.saveSettings({
            steam_api_key: key.value || undefined,
            steamid: steamid.value,
            allow_launch: launchBox.querySelector("input").checked,
            first_impression_minutes: Number(impression.value) || 12,
            hours_per_week: pace.value,
            store_cc: cc.value,
          });
          key.value = "";
          toast("Saved.");
          renderSettings(root, ctx);
        } catch (e) {
          toast(e.message);
        }
      },
    },
    "Save"
  );

  clear(root).append(
    h(
      "div",
      {},
      h("p", { class: "eyebrow" }, "Settings"),
      h("h1", { class: "display", style: "font-size:var(--step-3)" }, "Your machine, your data."),
      h(
        "dl",
        { class: "stat-grid" },
        h("div", { class: "stat" }, h("dt", {}, "In the library"), h("dd", {}, s.library.games)),
        h("div", { class: "stat" }, h("dt", {}, "Identified"), h("dd", {}, s.library.enriched)),
        h("div", { class: "stat" }, h("dt", {}, "Installed"), h("dd", {}, s.library.installed)),
        h("div", { class: "stat" }, h("dt", {}, "Artwork cached"), h("dd", {}, s.artwork_cached))
      ),
      h(
        "p",
        { class: "keyhint" },
        s.steam_root ? `Steam client: ${s.steam_root}` : "No local Steam client found.",
        " · Data: ",
        s.data_dir,
        " · Last sync: ",
        s.last_sync ? ago(s.last_sync) : "never"
      ),
      h(
        "div",
        { class: "row", style: "margin:1.4rem 0 2.4rem" },
        h(
          "button",
          {
            class: "btn",
            type: "button",
            onclick: async () => {
              await api.sync(false);
              toast("Syncing. It keeps going if you close this tab.");
            },
          },
          "Sync now"
        ),
        h(
          "button",
          {
            class: "btn ghost",
            type: "button",
            onclick: async () => {
              await api.sync(true);
              toast("Full re-read started — every game gets re-identified.");
            },
          },
          "Full resync"
        ),
        h("a", { class: "btn ghost", href: "/api/export" }, "Export my data"),
        h("a", { class: "btn ghost", href: "#/history" }, "Session log")
      ),
      h("hr", { class: "rule" }),
      h("h2", { style: "font-weight:400;font-size:var(--step-2)" }, "The ritual"),
      field(
        "First-impression checkpoint (minutes)",
        impression,
        "How long tonight waits before asking whether the game is actually working. Twelve minutes is enough to know."
      ),
      field(
        "Hours you play in a week",
        pace,
        "Used for the backlog arithmetic. Leave blank and tonight infers it from Steam's two-week counter."
      ),
      launchBox,
      h("hr", { class: "rule" }),
      h("h2", { style: "font-weight:400;font-size:var(--step-2)" }, "Steam"),
      h(
        "p",
        { class: "muted", style: "max-width:46ch" },
        "tonight reads your installed Steam client directly, so it works with no account details at all. A Web API key adds the games you own but have never installed."
      ),
      field("Steam Web API key", key, h("span", {}, "Get one at steamcommunity.com/dev/apikey. Stored only in your own database, never sent anywhere but Valve.")),
      field("SteamID64", steamid, "Detected from your Steam client when possible."),
      field("Store country code", cc, "Affects prices and release dates pulled from the storefront."),
      h("div", { class: "row" }, save)
    )
  );
}
