async function request(path, options = {}) {
  const res = await fetch(path, {
    headers: options.body ? { "content-type": "application/json" } : {},
    ...options,
  });
  const text = await res.text();
  let data = null;
  if (text) {
    try {
      data = JSON.parse(text);
    } catch {
      throw new Error(`Server sent something unreadable (${res.status})`);
    }
  }
  if (!res.ok) throw new Error((data && data.error) || `Request failed (${res.status})`);
  return data;
}

const get = (path) => request(path);
const post = (path, body) => request(path, { method: "POST", body: JSON.stringify(body ?? {}) });

export const api = {
  state: () => get("/api/state"),
  saveSettings: (body) => post("/api/settings", body),
  sync: (full = false) => post("/api/sync", { full }),
  cancelSync: () => post("/api/sync/cancel"),

  deal: (body) => post("/api/deal", body),
  fit: (body) => post("/api/fit", body),
  hand: (id) => get(`/api/hand/${id}`),
  pass: (id, appid) => post(`/api/hand/${id}/pass`, { appid }),
  rebound: (body) => post("/api/rebound", body),

  session: () => get("/api/session"),
  startSession: (body) => post("/api/session/start", body),
  impression: (id, answer) => post("/api/session/impression", { id, answer }),
  endSession: (id, verdict, reason) => post("/api/session/end", { id, verdict, reason }),
  sessions: () => get("/api/sessions"),
  launch: (appid) => post("/api/launch", { appid }),

  duel: (exclude, round, of) =>
    get(`/api/duel?exclude=${encodeURIComponent(exclude.join(","))}&round=${round}&of=${of}`),
  duelResult: (winner, loser) => post("/api/duel", { winner, loser }),

  reckoning: () => get("/api/reckoning"),
  amnestyCandidates: () => get("/api/amnesty/candidates"),
  amnesty: (appid, on = true) => post("/api/amnesty", { appid, on }),
  pin: (appid, on = true) => post("/api/pin", { appid, on }),

  game: (appid) => get(`/api/game/${appid}`),
  setSessionLength: (appid, minutes) => post(`/api/game/${appid}/session-length`, { minutes }),
  setTrait: (appid, name, value) => post(`/api/game/${appid}/trait`, { name, value }),

  shelves: () => get("/api/shelves"),
  shelf: (id, limit = 60) => get(`/api/shelf/${id}?limit=${limit}`),
  search: (q) => get(`/api/search?q=${encodeURIComponent(q)}`),
  taste: () => get("/api/taste"),
};
