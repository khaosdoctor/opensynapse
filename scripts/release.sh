#!/usr/bin/env bash
# Bumps the version everywhere a release needs it (Cargo.toml, Cargo.lock, the
# AUR PKGBUILDs), runs the tests, commits, tags and pushes. The tag starts
# release.yml. device-release.yml uses it too, with --no-push.
#
# usage: scripts/release.sh <X.Y.Z|patch|minor> [--no-push]
set -euo pipefail
cd "$(dirname "$0")/.."

die() { echo "release: $*" >&2; exit 1; }

want=${1:-}
[[ -n $want ]] || die "usage: scripts/release.sh <X.Y.Z|patch|minor> [--no-push]"
push=true
[[ ${2:-} == --no-push ]] && push=false

if $push; then
  [[ $(git branch --show-current) == main ]] || die "not on main"
  [[ -z $(git status --porcelain) ]] || die "commit or stash your changes first"
  git pull -q --rebase
fi

old=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
IFS=. read -r major minor patch <<< "$old"
case $want in
  patch) new="$major.$minor.$((patch + 1))" ;;
  minor) new="$major.$((minor + 1)).0" ;;
  *) new=$want ;;
esac
[[ $new =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "not a version: $new"
git rev-parse -q --verify "refs/tags/v$new" >/dev/null && die "v$new already exists"

sed -i "s/^version = \"$old\"/version = \"$new\"/" Cargo.toml
sed -i "s/^pkgver=.*/pkgver=$new/" contrib/aur/*/PKGBUILD
cargo update -q --workspace
cargo test -q --locked

git commit -qam "chore: release v$new"
git tag "v$new"
if ! $push; then
  echo "tagged v$new, push with: git push origin main v$new"
  exit 0
fi
git push -q origin main "v$new"
echo "released v$new, release.yml is building it"
