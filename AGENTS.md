# Working on tonight

Read this before changing anything. `README.md` covers what the product *is*; this
covers how it is built and which invariants are load-bearing.

## Shape of the code

```
src/
  main.rs        bootstrap, embedded static assets, graceful shutdown
  config.rs      CLI/env, data dir, Steam root detection across platforms
  db.rs          SQLite schema + connection mutex. Never await while holding it.
  model.rs       the vocabulary: Game, Meta, Shape, Card, Hand, Slot
  steam/
    appinfo.rs   binary appinfo.vdf parser: names, types, tags, categories, audio, Deck
    catalog.rs   GENERATED tag/category/genre id tables (scripts/gen-catalog.py)
    vdf.rs       text KeyValues parser (localconfig, libraryfolders, acf)
    local.rs     credential-free scan of the Steam client
    web.rs       optional Web API + public storefront enrichment
    images.rs    artwork proxy with a disk cache
  engine/
    signals.rs   tag/category/audio/Deck evidence → weighted votes on eleven axes
    shape.rs     genre/keyword/tag → session shape and the eleven traits
    conditions.rs the evening's circumstances: definitions, per-game fits, wording
    score.rs     the explainable scoring function; desire × condition gate
    deal.rs      conditions first, relaxation ladder, slot triad, draw, passes, rerolls
    overrides.rs per-game trait overrides the player has set
    taste.rs     Elo duels + per-dimension taste weights
    reckoning.rs backlog arithmetic and amnesty candidates
    shelves.rs   question-shaped shelves and search
    session.rs   the clock, the checkpoint, the verdict
  sync.rs        the background ingest pipeline with SSE progress
  api.rs         axum routes
web/             no build step. Plain ES modules, one stylesheet.
```

## Invariants

- **Never show more than three candidates.** The product is the constraint. Any
  feature that lets a user scroll a list of everything they own is a regression.
- **Every score component that fires must be able to explain itself in a
  sentence.** `score.rs` returns `(weight, text)` pairs, and the card shows the
  strongest one. A silent heuristic is a bug.
- **Passing must cost something.** Two passes per hand, and the last card cannot be
  passed. Rerolls are counted and shown.
- **Bailing out is a first-class outcome**, not a failure state. It feeds the taste
  model just as strongly as loving something.
- **No credentials required.** Anything that only works with an API key must
  degrade cleanly to the local-client path.
- **The browser never talks to Valve.** Artwork goes through `/img/{appid}/{kind}`.
- **Conditions are checked before taste.** A condition (sound taken, interrupted,
  couch, offline, installed, scary) decides who may be dealt at all and scales the
  score as a gate. Taste only orders the survivors. Never fold a condition into the
  additive score.
- **Unknown is not safe.** Read a trait through `Trait::assume(fallback)`, never
  `.value` directly, when deciding whether a game passes a condition. Low confidence
  must pull toward the pessimistic fallback.
- **Loosening is announced.** When the relaxation ladder steps down, `Hand.relaxed`
  says which condition was loosened. When nothing survives, `Deck.blame` names the
  condition that cost the most. A silent fallback is a bug.
- **Every active condition shows on every card** as a `Fit` with a sentence.
- **Learning knows why.** A bail-out carries a reason; "I couldn't hear it" must not
  count against the game like "it was bad" does.

## Gotchas

- `meta` holds two sources: the storefront crawl (`genres`, `categories`,
  `metacritic`, `release_year`, descriptions, prices) and the local client cache
  (`tags`, `l_genres`, `l_categories`, `l_metacritic`, `l_release_year`, audio, Deck,
  reviews). `load::candidates` merges them; neither pipeline may write the other's
  columns. The local scan reruns at start and every two minutes.
- New columns go in `db::COLUMNS`, never in `SCHEMA`, so existing databases migrate.
- Hands are stored as JSON. Any field added to `Card` needs `#[serde(default)]`, or
  every hand dealt before the upgrade fails to load.
- `appinfo.vdf` category ids and genre ids have no public bulk endpoint. Names live in
  `scripts/gen-catalog.py`; unknown ids are dropped, never guessed.
- Tag votes are keyed by lower-cased tag name in `signals::tag_votes`. A tag with no
  entry votes for nothing, which is the safe default.
- The SIGTERM handler in `main.rs` matters: PID 1 in a container ignores SIGTERM
  unless a handler is installed, so `docker stop` would otherwise hang for ten seconds.
- `Db::with` holds a `std::sync::Mutex`. Do not `await` inside the closure.
- `appinfo.vdf` is version-tagged: `0x07564427/28/29`. v29 has a trailing string
  table and keys are `u32` indices into it; older versions inline the key strings.
  Steam rewrites this file, so parse defensively and treat failure as "no names".
- `localconfig.vdf` stores `LastPlayed` `86400` as a sentinel for "never", not as
  a real timestamp.
- `store.steampowered.com/api/appdetails` is rate-limited around 200 requests per
  5 minutes. The sync loop paces at 1.4s and backs off on 429/403; do not
  parallelise it.
- `ISteamApps/GetAppList` no longer exists (it 404s). Names come from `appinfo.vdf`
  or from per-app storefront lookups. Do not reintroduce the bulk endpoint.
- Session shape falls back to a genre guess, but the player's own median session
  length wins once two sessions are logged, and a manual override wins outright.

## Tests

`cargo test` covers the VDF and appinfo parsers (including a synthetic cache, so
tags and audio decode in CI), the trait evidence model, every condition, the time-fit
curve, and the dealer: that sound-taken never deals a game that needs sound, that a
thin library loosens and says so, that an impossible evening names its culprit, and
that hands are deterministic for a seed but vary across seeds. `scripts/e2e.sh` drives
a running instance through the whole API.

## Style

No inline comments. If something needs explaining, it goes in a `///` doc comment
on a well-named function.
