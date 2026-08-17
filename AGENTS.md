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
    appinfo.rs   binary appinfo.vdf parser — offline names and app types
    vdf.rs       text KeyValues parser (localconfig, libraryfolders, acf)
    local.rs     credential-free scan of the Steam client
    web.rs       optional Web API + public storefront enrichment
    images.rs    artwork proxy with a disk cache
  engine/
    shape.rs     genre/keyword → session shape (the session-length model)
    score.rs     the explainable scoring function
    deal.rs      slot-based triad selection, passes, rerolls
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

## Gotchas

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

`cargo test` covers the VDF parsers, the session-shape model, and the time-fit
curve. The `appinfo` test is a no-op when no Steam client is present, so it stays
green in CI.

## Style

No inline comments. If something needs explaining, it goes in a `///` doc comment
on a well-named function.
