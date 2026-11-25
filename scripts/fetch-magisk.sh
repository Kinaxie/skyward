#!/bin/zsh
# Refresh the Magisk APK carried into the recovery from the newest release,
# or from SKYWARD_MAGISK_TAG. A build uses the committed APK unless
# SKYWARD_MAGISK_LATEST=1 asks build.sh for this.
set -euo pipefail
cd "$(dirname "$0")/.."

repo=${SKYWARD_MAGISK_REPO:-topjohnwu/Magisk}
tag=${SKYWARD_MAGISK_TAG:-latest}
dest=crates/skyward-root/assets/magisk
api_opts=(-fsSL --retry 3 --retry-delay 2 --noproxy '*')
curl_opts=(-fsSL --retry 3 --retry-delay 2)
if [[ -n "${SKYWARD_MAGISK_PROXY:-}" ]]; then
  curl_opts+=(-x "$SKYWARD_MAGISK_PROXY")
fi
if [[ -n "${SKYWARD_MAGISK_API_PROXY:-}" ]]; then
  api_opts+=(-x "$SKYWARD_MAGISK_API_PROXY")
fi
if [[ -n "${GITHUB_TOKEN:-}" ]]; then
  api_opts+=(-H "Authorization: Bearer $GITHUB_TOKEN")
fi

print "fetching $repo release $tag"
release_file=$(mktemp)
trap 'rm -f "$release_file"' EXIT
curl "${api_opts[@]}" -o "$release_file" "https://api.github.com/repos/$repo/releases/$tag"
version=$(node -e 'const fs=require("fs"); console.log(JSON.parse(fs.readFileSync(process.argv[1], "utf8")).tag_name)' "$release_file")
asset=$(node -e 'const fs=require("fs"); const j=JSON.parse(fs.readFileSync(process.argv[1], "utf8")); const a=(j.assets||[]).find(a=>/^Magisk-v.*\.apk$/.test(a.name)); if(a) console.log(a.name)' "$release_file")
[[ -n "$asset" ]] || { print -u2 "the release names no Magisk APK"; exit 1; }

mkdir -p "$dest"
print "  $asset"
curl "${curl_opts[@]}" -o "$dest/Magisk.apk" "https://github.com/$repo/releases/download/$version/$asset"
curl "${curl_opts[@]}" -o "$dest/LICENSE" "https://raw.githubusercontent.com/$repo/$version/LICENSE"
printf '%s' "$version" > "$dest/VERSION"
print "Magisk $version is in $dest"
