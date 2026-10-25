#!/usr/bin/env bash
# End-to-end API check against a running tonight instance.
#   cargo run --release -- -p 7788 &
#   TONIGHT_URL=http://localhost:7788 scripts/e2e.sh
set -u
B=${TONIGHT_URL:-http://localhost:7777}
pass=0; fail=0
chk() { # name, expr
  if eval "$2" >/dev/null 2>&1; then printf "  ok   %s\n" "$1"; pass=$((pass+1));
  else printf "  FAIL %s\n" "$1"; fail=$((fail+1)); fi
}
j() { curl -s -X POST "$B$1" -H 'content-type: application/json' -d "$2"; }
g() { curl -s "$B$1"; }

echo "== deal / pass / lock"
HAND=$(j /api/deal '{"budget":45,"mood":"mellow","appetite":"surprise"}')
ID=$(echo "$HAND" | jq -r .id)
N=$(echo "$HAND" | jq '.cards|length')
chk "hand has 3 cards" "[ $N -eq 3 ]"
chk "hand id present" "[ -n '$ID' ]"
A1=$(echo "$HAND" | jq -r '.cards[0].appid')
A2=$(echo "$HAND" | jq -r '.cards[1].appid')
A3=$(echo "$HAND" | jq -r '.cards[2].appid')
H2=$(j "/api/hand/$ID/pass" "{\"appid\":$A1}")
chk "pass 1 leaves 2" "[ $(echo "$H2" | jq '.cards|length') -eq 2 ]"
H3=$(j "/api/hand/$ID/pass" "{\"appid\":$A2}")
chk "pass 2 leaves 1" "[ $(echo "$H3" | jq '.cards|length') -eq 1 ]"
chk "hand is locked" "[ $(echo "$H3" | jq -r .locked) = true ]"
H4=$(j "/api/hand/$ID/pass" "{\"appid\":$A3}")
chk "cannot pass the last card" "[ $(echo "$H4" | jq '.cards|length') -eq 1 ]"

echo "== reroll excludes what you rejected"
R=$(j /api/deal "{\"budget\":45,\"mood\":\"mellow\",\"appetite\":\"surprise\",\"reroll_of\":\"$ID\",\"exclude\":[$A1,$A2,$A3]}")
chk "reroll note shown" "[ $(echo "$R" | jq -r '.note != null') = true ]"
chk "reroll count is 1" "[ $(echo "$R" | jq .rerolls) -eq 1 ]"
OVERLAP=$(echo "$R" | jq "[.cards[].appid] | map(select(. == $A1 or . == $A2 or . == $A3)) | length")
chk "reroll excludes prior cards" "[ $OVERLAP -eq 0 ]"

echo "== session lifecycle"
S=$(j /api/session/start "{\"appid\":$A3,\"hand_id\":\"$ID\",\"budget\":45,\"mood\":\"mellow\",\"appetite\":\"surprise\"}")
SID=$(echo "$S" | jq -r .session.id)
chk "session started" "[ -n '$SID' ] && [ '$SID' != null ]"
chk "current session matches" "[ $(g /api/session | jq -r .session.id) = '$SID' ]"
chk "launch url is a steam url" "g /api/session | jq -re '.session.launch_url' | grep -q '^steam://rungameid/'"
chk "impression recorded" "j /api/session/impression '{\"id\":$SID,\"answer\":\"in\"}' | jq -e .ok"
chk "session ends with verdict" "j /api/session/end '{\"id\":$SID,\"verdict\":\"loved\"}' | jq -e .ok"
chk "no session after end" "[ $(g /api/session | jq -r '.session') = null ]"
chk "session shows in the log" "g /api/sessions | jq -e '.sessions[0].verdict == \"loved\"'"

echo "== only one session on the clock"
S1=$(j /api/session/start "{\"appid\":$A1,\"budget\":45,\"mood\":\"mellow\",\"appetite\":\"surprise\"}")
S2=$(j /api/session/start "{\"appid\":$A2,\"budget\":45,\"mood\":\"mellow\",\"appetite\":\"surprise\"}")
chk "second start supersedes the first" "[ $(g /api/session | jq -r .session.appid) -eq $A2 ]"
j /api/session/end "{\"id\":$(echo "$S2" | jq -r .session.id),\"verdict\":\"fine\"}" >/dev/null

echo "== rebound"
chk "rebound returns a card" "j /api/rebound \"{\\\"budget\\\":45,\\\"mood\\\":\\\"fried\\\",\\\"appetite\\\":\\\"surprise\\\",\\\"exclude\\\":[$A1]}\" | jq -e .appid"

echo "== duels"
D=$(g "/api/duel?exclude=&round=1&of=7")
W=$(echo "$D" | jq -r .a.appid); L=$(echo "$D" | jq -r .b.appid)
chk "duel returns two sides" "[ -n '$W' ] && [ -n '$L' ] && [ '$W' != '$L' ]"
chk "duel result accepted" "j /api/duel \"{\\\"winner\\\":$W,\\\"loser\\\":$L}\" | jq -e .ok"
chk "elo moved for the winner" "g /api/game/$W | jq -e '.elo > 1500'"
chk "taste profile has entries" "[ $(g /api/taste | jq '.taste|length') -gt 0 ]"

echo "== amnesty and pinning"
chk "retire a game" "j /api/amnesty \"{\\\"appid\\\":$L,\\\"on\\\":true}\" | jq -e .ok"
chk "retired game is on the retired shelf" "g /api/shelf/retired | jq -e \"[.items[].appid] | index($L) != null\""
seen_retired=0
for i in 1 2 3 4 5 6 7 8; do
  if j /api/deal '{"budget":45,"mood":"mellow","appetite":"surprise"}' | jq -e "[.cards[].appid] | index($L) != null" >/dev/null 2>&1; then
    seen_retired=$((seen_retired+1))
  fi
done
chk "retired game is never dealt" "[ $seen_retired -eq 0 ]"
chk "un-retire" "j /api/amnesty \"{\\\"appid\\\":$L,\\\"on\\\":false}\" | jq -e .ok"
chk "pin a game" "j /api/pin \"{\\\"appid\\\":$W,\\\"on\\\":true}\" | jq -e .ok"
chk "unpin" "j /api/pin \"{\\\"appid\\\":$W,\\\"on\\\":false}\" | jq -e .ok"
chk "amnesty candidates offered" "[ $(g /api/amnesty/candidates | jq '.candidates|length') -gt 0 ]"

echo "== shelves and search"
for s in installed sealed abandoned nearly quick evening together ghosts loved passed expensive retired quiet pausable couch offline gentle deck; do
  chk "shelf $s responds" "g /api/shelf/$s | jq -e '.items|type == \"array\"'"
done
chk "search finds a known game" "g '/api/search?q=half-life' | jq -e '.items|length > 0'"
chk "search is empty for gibberish" "[ $(g '/api/search?q=zzzqqqxx' | jq '.items|length') -eq 0 ]"

echo "== manual session length override"
chk "set override" "j /api/game/$W/session-length '{\"minutes\":25}' | jq -e .ok"
chk "override is applied" "g /api/game/$W | jq -e '.shape.session_minutes == 25'"
chk "clear override" "j /api/game/$W/session-length '{\"minutes\":null}' | jq -e .ok"

echo "== conditions"
chk "state lists the conditions" "[ \$(g /api/state | jq '.conditions|length') -eq 8 ]"
chk "state counts playable games" "g /api/state | jq -e '.library.playable > 0'"
FIT0=$(j /api/fit '{"budget":0,"mood":"mellow","appetite":"surprise"}')
FITQ=$(j /api/fit '{"budget":0,"mood":"mellow","appetite":"surprise","conditions":["quiet"]}')
chk "fit counts the library" "[ $(echo "$FIT0" | jq .viable) -gt 0 ]"
chk "sound taken narrows the pool" "[ $(echo "$FITQ" | jq .viable) -lt $(echo "$FIT0" | jq .viable) ]"
chk "unknown conditions are ignored" "[ $(j /api/fit '{"conditions":["nonsense"]}' | jq .viable) -eq $(echo "$FIT0" | jq .viable) ]"
HQ=$(j /api/deal '{"budget":0,"mood":"mellow","appetite":"surprise","conditions":["quiet","gentle"]}')
chk "conditioned hand has cards" "[ $(echo "$HQ" | jq '.cards|length') -gt 0 ]"
chk "every card carries its condition fits" "printf %s \"\$HQ\" | jq -e '[.cards[] | (.fits|map(.id)|sort) == [\"gentle\",\"quiet\"]] | all'"
chk "no card fails sound taken badly" "printf %s \"\$HQ\" | jq -e '[.cards[].fits[] | select(.id==\"quiet\") | .value >= 0.1] | all'"
chk "hand remembers its conditions" "g /api/hand/$(echo "$HQ" | jq -r .id) | jq -e '.conditions == [\"quiet\",\"gentle\"]'"
chk "hand reports the pool it was dealt from" "printf %s \"\$HQ\" | jq -e '.viable > 0 and .library >= .viable'"
CA=$(echo "$HQ" | jq -r '.cards[0].appid')
chk "card breaks its score into parts" "printf %s \"\$HQ\" | jq -e '.cards[0].parts|length > 0'"
chk "game detail returns a fit per condition" "[ $(g /api/game/$CA | jq '.fits|length') -eq 8 ]"
chk "game detail returns tags" "g /api/game/$CA | jq -e '.meta.tags|type == \"array\"'"
SC=$(j /api/session/start "{\"appid\":$CA,\"budget\":0,\"mood\":\"mellow\",\"appetite\":\"surprise\",\"conditions\":[\"quiet\"]}")
chk "session keeps its conditions" "printf %s \"\$SC\" | jq -e '.session.conditions == [\"quiet\"]'"
chk "bailing for sound teaches the engine" "j /api/session/end \"{\\\"id\\\":$(echo "$SC" | jq -r .session.id),\\\"verdict\\\":\\\"bounced\\\",\\\"reason\\\":\\\"sound\\\"}\" | jq -e .ok"
chk "the sound override was recorded" "g /api/game/$CA | jq -e '.overrides.sound == 0.85'"
chk "log shows the reason" "g /api/sessions | jq -e '.sessions[0].reason == \"Needed the sound\"'"
chk "clear the override" "j /api/game/$CA/trait '{\"name\":\"sound\",\"value\":null}' | jq -e .ok"
chk "override is gone" "g /api/game/$CA | jq -e '(.overrides.sound // null) == null'"
chk "unknown trait is refused" "j /api/game/$CA/trait '{\"name\":\"nonsense\",\"value\":0.5}' | jq -e .error"
chk "rebound honours conditions" "j /api/rebound '{\"budget\":0,\"mood\":\"fried\",\"appetite\":\"surprise\",\"conditions\":[\"quiet\"],\"exclude\":[]}' | jq -e '.fits|length == 1'"
chk "vault shelves are grouped" "g /api/shelves | jq -e '[.shelves[].group] | all(. != null)'"
chk "library readiness is reported" "g /api/reckoning | jq -e '.readiness|length == 7'"

echo "== reckoning, export, artwork, guards"
chk "reckoning computes" "g /api/reckoning | jq -e '.owned > 0'"
chk "export is a download" "curl -s -D- -o /dev/null $B/api/export | grep -qi 'content-disposition'"
chk "export contains events" "g /api/export | jq -e '.events|length > 0'"
chk "artwork proxied from cache" "curl -s -o /dev/null -w '%{content_type}' $B/img/220/capsule | grep -q image"
chk "launch refused while switched off" "j /api/launch '{\"appid\":220}' | jq -e .error"
chk "unknown game 400s" "j /api/deal '{}' | jq -e '.cards|length > 0'"
chk "bad hand id errors cleanly" "g /api/hand/nope | jq -e .error"
chk "spa fallback serves the app" "curl -s $B/anything | grep -q '<title>tonight</title>'"
chk "css is served" "curl -s -o /dev/null -w '%{http_code}' $B/app.css | grep -q 200"

echo
echo "passed: $pass   failed: $fail"
[ "$fail" -eq 0 ]
