# tonight

**A decision engine for your Steam library.** Three cards, two passes, one game.

You own hundreds of games and you play the same four. The problem was never that
you lack options — it is that an infinite grid of boxart is a decision engine
running in reverse. `tonight` deletes the grid. It asks you three questions, deals
you exactly three games, and takes two of them away.

Self-hosted, single binary, SQLite, no account, no telemetry. It reads your local
Steam client directly, so it works with **no API key and no credentials at all**.

---

## Install

One command. No Rust, no Docker, no terminal expertise. It downloads the right
binary, sets itself to start with your login, and opens the table.

**Linux / macOS**

```sh
curl -fsSL https://github.com/guitaripod/tonight/releases/latest/download/install.sh | sh
```

**Windows** (PowerShell — right-click the Start menu → Terminal)

```powershell
irm https://github.com/guitaripod/tonight/releases/latest/download/install.ps1 | iex
```

That is the whole setup — it finds your Steam install, reads it, and starts
identifying your library in the background. Binaries are attached to every
[release](https://github.com/guitaripod/tonight/releases) too, if you would rather
click a download than pipe a script.

### Advanced

- **From source** — clone, `cargo build --release`, `./target/release/tonight --open`.
- **Docker** — `docker compose up -d`.
- **As a systemd service** — or just run `install.sh` with a user session, which
  sets this up for you. The unit is in `packaging/tonight.service`.

## The table

Three questions, then the hand. Cards land face-down and turn over. You choose a
kind of evening — a sure thing, unfinished business, or a stranger — never a wall
of boxart.

| The deal | The table afterwards |
| --- | --- |
| ![The three questions and the deal](docs/screens/table.png) | ![A dealt hand on the table](docs/screens/hand.png) |

Every recommendation states its reasoning in a sentence you can argue with —
*"You stopped 40 minutes in, 9 months ago. That was never a fair trial."*

---

## The thesis

Choice paralysis is not an information problem, so more information does not fix
it. Every complaint people have about their backlog is one of six things, and each
one is a design decision here:

| The complaint | What `tonight` does |
| --- | --- |
| "Too many options, I can't compare them" | Never shows more than three, and the three are deliberately incomparable: a **sure thing**, an **unfinished business**, and a **stranger**. You are choosing a kind of evening, not a game. |
| "I browse for 40 minutes and then watch YouTube" | Passing is limited. Two passes and the last card is the verdict — there is no third pass. Re-dealing is allowed, counted, and told to your face. |
| "I don't know if I have time for it" | Every game gets a **session shape** — how long one sitting actually is — derived from genre, categories and description, then corrected by how long *you* actually play it. A 20-minute slot never offers you a grand strategy game. |
| "I don't know what I'm in the mood for" | Five moods map to five axes (pace, thinking, story, company, calm). The engine matches the game's shape to your state. |
| "I feel guilty about the ones I never played" | **Amnesty.** Retire a game permanently, in one tap, with the hours it removes from your backlog counted for you. A backlog is only a debt if you agree to owe it. |
| "I bounce off everything and feel bad" | The **first-impression checkpoint**: twelve minutes in, `tonight` asks whether it is working. Bailing is a first-class, logged, guilt-free answer — and it immediately deals you a replacement. |

Every recommendation states its reasoning in a sentence you can argue with:
*"You stopped 40 minutes in, 9 months ago. That was never a fair trial."*

## Screens

- **The table** — the three questions, then the hand. Cards land face-down and turn over.
- **This or that** — seven forced pairwise choices. Cheap for you, dense for the model; Elo plus per-genre weights.
- **The reckoning** — the honest arithmetic. How many you have never opened, what
  the unopened ones cost, and the year you would finish at your real pace. Then
  the amnesty flow, which offers the least likely candidates three at a time.
- **The vault** — search-only. There is deliberately no browsable wall of boxart
  anywhere in this app; instead there are shelves that are questions
  ("You walked out early", "Ghosts", "If someone's over").

## Where the data comes from

| Source | Needs | Gives |
| --- | --- | --- |
| `appcache/appinfo.vdf` | nothing | Name and type of every app your client knows — parsed offline from Steam's own binary cache |
| `userdata/*/config/localconfig.vdf` | nothing | Minutes played and last-played, per game |
| `steamapps/appmanifest_*.acf` | nothing | What is installed, and how big it is |
| `store.steampowered.com/api/appdetails` | nothing | Genres, categories, Metacritic, price, release date |
| `IPlayerService/GetOwnedGames` | a Web API key | Games you own but never installed on this machine |
| `ISteamUserStats/GetPlayerAchievements` | a Web API key | How close you are to finishing something |

The key is optional and only ever leaves your machine to talk to Valve. Artwork is
proxied and cached to disk, so your browser never contacts Valve at all and a
warmed instance works offline.

## Options

```
-b, --bind <ADDR>   Listen address (default 0.0.0.0:7777, env TONIGHT_BIND)
-p, --port <PORT>   Shorthand for --bind 0.0.0.0:<PORT>
-d, --data <DIR>    Data directory (env TONIGHT_DATA_DIR)
    --steam <DIR>   Steam root, e.g. ~/.steam/steam (env TONIGHT_STEAM_ROOT)
    --open          Open a browser once the server is up
```

Everything else lives in Settings: the API key, the checkpoint length, your weekly
hours, and whether the server may start games on its own machine.

### Launching from the sofa

If `tonight` runs on your gaming PC, turn on **"Let this server start games on its
own machine"** and picking a card on your phone launches the game on the PC. It is
off by default, because it runs a command on the host.

Without it, `tonight` hands your browser a `steam://rungameid/<appid>` link, which
works whenever the browser is on the same machine as Steam.

## Keyboard

| Key | Does |
| --- | --- |
| `D` | Deal |
| `1` `2` `3` | Play that card |
| `Q` `W` `E` | Pass that card |
| `/` | Search the vault |
| `X` | End the session |
| `?` | Remind you of all this |

## Your data

`~/.local/share/tonight/tonight.db` — one SQLite file. `Export my data` in Settings
gives you every judgement you have made as JSON. Nothing is sent anywhere.

## Licence

GPL-3.0-only.
