#!/usr/bin/env bash
# Decides whether release/<version> is merged into main once its installers are built.
#   goes-to-main.sh <version>
# Prints two lines, ready for $GITHUB_OUTPUT:
#   to_main=true|false
#   reason=<why>
# Needs GH_REPO (owner/name) and a GH_TOKEN that can read the repository's tags.
#
# The rules, in order:
# - a final version (no -alpha, -beta, -rc...) goes to main;
# - a release candidate (-rc...) goes to main;
# - other pre-releases (-alpha, -beta...) go to main while PRERELEASES_TO_MAIN is true;
# - any version goes to main when a tag to-main/<version> or main/<version> exists. It can point at
#   any commit (develop or release/<version>) and be pushed at any time: before the pull request,
#   while it is open or after the build (then to-main.yml merges at once).
set -euo pipefail

version="${1:?usage: goes-to-main.sh <version>}"

# TODO: set to false once only final versions and release candidates go to main; alpha and beta
# versions will then need a tag to-main/<version>.
PRERELEASES_TO_MAIN="${PRERELEASES_TO_MAIN:-true}"

to_main=false
reason="pre-release: push a tag to-main/$version to take it to main"
if [[ "$version" != *-* ]]; then
  to_main=true
  reason="final version"
elif [[ "$version" == *-rc* ]]; then
  to_main=true
  reason="release candidate"
elif [[ "$PRERELEASES_TO_MAIN" == true ]]; then
  to_main=true
  reason="pre-release, while PRERELEASES_TO_MAIN is true"
fi

for tag in "to-main/$version" "main/$version"; do
  if gh api "repos/$GH_REPO/git/ref/tags/$tag" --silent 2> /dev/null; then
    to_main=true
    reason="tag $tag"
    break
  fi
done

echo "to_main=$to_main"
echo "reason=$reason"
