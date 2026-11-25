#!/bin/zsh
# Refresh the KernelSU payloads carried into the recovery from the newest
# KernelSU release, or from the tag SKYWARD_KSU_TAG names. The build uses the
# files already in assets/ unless SKYWARD_KSU_LATEST=1 asks for this.
#
# A network that cannot reach github.com directly can set HTTPS_PROXY for the
# release files, or SKYWARD_KSU_PROXY to the proxy's URL. GitHub's API is
# asked directly unless SKYWARD_KSU_API_PROXY names one, because a proxy that
# carries the release files does not always carry the API.
set -euo pipefail
cd "$(dirname "$0")/.."

repo=${SKYWARD_KSU_REPO:-tiann/KernelSU}
tag=${SKYWARD_KSU_TAG:-latest}
api="https://api.github.com/repos/$repo/releases/$tag"
dest=crates/skyward-root/assets/kernelsu
curl_opts=(-fsSL --retry 3 --retry-delay 2)
api_opts=(-fsSL --retry 3 --retry-delay 2 --noproxy '*')
if [[ -n "${SKYWARD_KSU_PROXY:-}" ]]; then
  curl_opts+=(-x "$SKYWARD_KSU_PROXY")
fi
if [[ -n "${SKYWARD_KSU_API_PROXY:-}" ]]; then
  api_opts+=(-x "$SKYWARD_KSU_API_PROXY")
fi
if [[ -n "${GITHUB_TOKEN:-}" ]]; then
  api_opts+=(-H "Authorization: Bearer $GITHUB_TOKEN")
fi

print "fetching $repo release $tag"
release_file=$(mktemp)
trap 'rm -f "$release_file"' EXIT
curl "${api_opts[@]}" -o "$release_file" "$api"
version=$(node -e 'const fs=require("fs"); console.log(JSON.parse(fs.readFileSync(process.argv[1], "utf8")).tag_name)' "$release_file")
assets=()
while IFS= read -r name; do
  [[ -n "$name" ]] && assets+=("$name")
done < <(node -e 'const fs=require("fs"); const j=JSON.parse(fs.readFileSync(process.argv[1], "utf8")); for (const a of j.assets || []) if (/^lkm-aarch64-.*_kernelsu\.ko$/.test(a.name) || a.name === "ksud-aarch64-linux-android" || /^KernelSU_.*release\.apk$/.test(a.name)) console.log(a.name)' "$release_file")
[[ ${#assets[@]} -gt 0 ]] || { print -u2 "the release names no aarch64 LKM or ksud"; exit 1; }

mkdir -p "$dest"
rm -f "$dest"/lkm-aarch64-*_kernelsu.ko "$dest"/ksud-aarch64-linux-android "$dest"/KernelSU.apk
for name in $assets; do
  out=$name
  [[ $name == *.apk ]] && out=KernelSU.apk
  print "  $name"
  curl "${curl_opts[@]}" -o "$dest/$out" "https://github.com/$repo/releases/download/$version/$name"
done
curl "${curl_opts[@]}" -o "$dest/LICENSE" "https://raw.githubusercontent.com/$repo/$version/LICENSE"
printf '%s' "$version" > "$dest/VERSION"
print "KernelSU $version is in $dest"
