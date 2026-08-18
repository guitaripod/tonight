#!/usr/bin/env sh
set -eu

REPO=guitaripod/tonight
VERSION="${TONIGHT_VERSION:-latest}"
DEST="${TONIGHT_PREFIX:-$HOME/.local/bin}"

case "$(uname -s)/$(uname -m)" in
  Linux/x86_64)   ASSET=tonight-linux-x86_64 ;;
  Darwin/arm64)   ASSET=tonight-darwin-aarch64 ;;
  Darwin/x86_64)  ASSET=tonight-darwin-x86_64 ;;
  *)
    echo "tonight has no binary for $(uname -s) $(uname -m) yet."
    echo "Build it from source: https://github.com/$REPO"
    exit 1 ;;
esac

URL="https://github.com/$REPO/releases/$VERSION/download/$ASSET"
mkdir -p "$DEST"
echo "Downloading $ASSET…"
rm -f "$DEST/tonight.new"
ok=0
for attempt in 1 2 3 4 5; do
  curl -fsSL --connect-timeout 10 --max-time 300 -o "$DEST/tonight.new" "$URL" && ok=1 && break
  sleep 2
done
if [ "$ok" != 1 ]; then
  echo "Download failed — check your connection and run this again."
  exit 1
fi
chmod +x "$DEST/tonight.new"
mv -f "$DEST/tonight.new" "$DEST/tonight"

IP="$(hostname -I 2>/dev/null | awk '{print $1}' || true)"

if [ "$(uname -s)" = Linux ] && command -v systemctl >/dev/null 2>&1 \
  && systemctl --user show-environment >/dev/null 2>&1; then
  mkdir -p "$HOME/.config/systemd/user"
  printf '%s\n' \
    '[Unit]' \
    'Description=tonight — a decision engine for your Steam library' \
    'After=network-online.target' \
    'Wants=network-online.target' \
    '' \
    '[Service]' \
    'Type=simple' \
    "ExecStart=$DEST/tonight" \
    'Restart=on-failure' \
    'RestartSec=5' \
    'Environment=TONIGHT_BIND=0.0.0.0:7777' \
    'NoNewPrivileges=true' \
    'PrivateTmp=true' \
    '' \
    '[Install]' \
    'WantedBy=default.target' > "$HOME/.config/systemd/user/tonight.service"
  systemctl --user daemon-reload
  systemctl --user enable tonight >/dev/null 2>&1 || true
  systemctl --user restart tonight
  echo "tonight is running as a service and comes back on every login."
elif [ "$(uname -s)" = Darwin ]; then
  mkdir -p "$HOME/Library/LaunchAgents"
  printf '%s\n' \
    '<?xml version="1.0" encoding="UTF-8"?>' \
    '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
    '<plist version="1.0">' \
    '<dict>' \
    '  <key>Label</key>' \
    '  <string>guitaripod.tonight</string>' \
    '  <key>ProgramArguments</key>' \
    '  <array>' \
    "    <string>$DEST/tonight</string>" \
    '  </array>' \
    '  <key>RunAtLoad</key>' \
    '  <true/>' \
    '  <key>KeepAlive</key>' \
    '  <true/>' \
    '</dict>' \
    '</plist>' > "$HOME/Library/LaunchAgents/guitaripod.tonight.plist"
  launchctl unload "$HOME/Library/LaunchAgents/guitaripod.tonight.plist" >/dev/null 2>&1 || true
  launchctl load -w "$HOME/Library/LaunchAgents/guitaripod.tonight.plist"
  echo "tonight is running and added to your login items."
else
  nohup "$DEST/tonight" --open >/dev/null 2>&1 &
  echo "tonight is running for this session (no systemd found)."
fi

xdg-open "http://localhost:7777" >/dev/null 2>&1 || true
open "http://localhost:7777" >/dev/null 2>&1 || true

echo ""
echo "  tonight is up.   http://localhost:7777"
if [ -n "${IP:-}" ]; then echo "  From your phone: http://$IP:7777"; fi
echo "  Stop browsing. Deal a hand."