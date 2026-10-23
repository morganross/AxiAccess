#!/usr/bin/env bash
# Run as the desktop user, with sudo available for the owned /Applications app.
# User pairing/configuration and all unrelated application data are retained.
set -euo pipefail
[[ $# == 0 ]] || { echo 'Uninstall accepts no arguments and retains user data.' >&2; exit 2; }
APP='/Applications/AxiAccess Sender.app'
[[ -d "$APP" && ! -L "$APP" ]] || { echo 'AxiAccess Sender is not installed at its owned path.' >&2; exit 1; }
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Contents/Info.plist")" == com.axiowl.axiaccess.sender ]]
codesign --verify --strict --deep "$APP"
codesign -d --verbose=4 "$APP" 2>&1 | grep -q 'TeamIdentifier=3726YMX2FS'
"$APP/Contents/MacOS/AxiAccessSender" --disable-login
# macOS ps comm reports the executable path. Match exact owned paths, never all
# processes named axiaccess; Python/Node are not needed for removal.
while read -r pid image; do
 case "$image" in
  "$APP/Contents/MacOS/axiaccess"|"$APP/Contents/MacOS/axiaccess-login"|"$APP/Contents/MacOS/AxiAccessSender")
   current="$(/bin/ps -p "$pid" -o comm= 2>/dev/null || true)"
   [[ "$current" != "$image" ]] || /bin/kill -TERM "$pid" 2>/dev/null || true
   ;;
 esac
done < <(/bin/ps -axo pid=,comm=)
# A fixed verified product path, never a path computed from user data.
sudo /bin/rm -rf -- '/Applications/AxiAccess Sender.app'
sudo /usr/sbin/pkgutil --forget com.axiowl.axiaccess.sender.component >/dev/null
echo 'Sender removed; private AxiAccess settings and pairing data retained. No IDE/provider data was touched.'
