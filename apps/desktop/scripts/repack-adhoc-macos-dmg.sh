#!/usr/bin/env bash

# Tauri leaves the outer app bundle unsigned when no Apple Developer identity
# is configured. Re-signing the complete bundle prevents Gatekeeper from
# classifying a structurally valid unsigned app as damaged.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "Usage: $0 <installer.dmg>" >&2
  exit 64
fi

dmg_path="$1"
if [ ! -f "$dmg_path" ]; then
  echo "DMG not found: $dmg_path" >&2
  exit 66
fi

work_root="$(mktemp -d "${TMPDIR:-/tmp}/devconduit-macos-dmg.XXXXXX")"
mount_dir="$work_root/mount"
stage_dir="$work_root/stage"
repacked_dmg="$work_root/repacked.dmg"
mounted=0

cleanup() {
  if [ "$mounted" -eq 1 ]; then
    hdiutil detach "$mount_dir" -quiet || true
  fi
}
trap cleanup EXIT

mkdir -p "$mount_dir" "$stage_dir"
hdiutil attach -readonly -nobrowse -mountpoint "$mount_dir" "$dmg_path" >/dev/null
mounted=1

app_path="$(find "$mount_dir" -maxdepth 1 -type d -name '*.app' -print -quit)"
if [ -z "$app_path" ]; then
  echo "No app bundle found in DMG: $dmg_path" >&2
  exit 65
fi

app_name="$(basename "$app_path")"
staged_app="$stage_dir/$app_name"
ditto "$app_path" "$staged_app"
hdiutil detach "$mount_dir" -quiet
mounted=0

codesign --force --deep --sign - "$staged_app"
codesign --verify --deep --strict --verbose=2 "$staged_app"
ln -s /Applications "$stage_dir/Applications"

volume_name="${app_name%.app}"
hdiutil create -volname "$volume_name" -srcfolder "$stage_dir" -format UDZO "$repacked_dmg" >/dev/null
mv "$repacked_dmg" "$dmg_path"
hdiutil verify "$dmg_path" >/dev/null

echo "Repacked unsigned macOS installer with a complete ad-hoc app signature: $dmg_path"
