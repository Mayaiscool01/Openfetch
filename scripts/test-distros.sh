#!/bin/sh
# Build and test OpenFetch inside real distro containers (needs docker or podman).
# Usage: scripts/test-distros.sh [image ...]
set -eu
ENGINE=$(command -v podman || command -v docker || { echo "need podman or docker" >&2; exit 1; })
IMAGES=${*:-"debian:stable ubuntu:24.04 archlinux:latest fedora:latest opensuse/tumbleweed:latest alpine:latest"}
ROOT=$(cd "$(dirname "$0")/.." && pwd)

for img in $IMAGES; do
  echo "=== $img"
  case $img in
    debian*|ubuntu*) setup='apt-get update -qq && apt-get install -y -qq cargo gcc >/dev/null' ;;
    archlinux*)      setup='pacman -Sy --noconfirm --needed rust gcc >/dev/null' ;;
    fedora*)         setup='dnf install -y -q cargo gcc >/dev/null' ;;
    opensuse*)       setup='zypper -q install -y cargo gcc >/dev/null' ;;
    alpine*)         setup='apk add --quiet cargo gcc musl-dev' ;;
    *) echo "no setup recipe for $img" >&2; continue ;;
  esac
  $ENGINE run --rm -v "$ROOT":/src:ro "$img" sh -c "
    $setup &&
    cp -r /src /build && cd /build &&
    cargo test --locked 2>&1 | grep -E 'test result|FAILED|error' ;
    cargo run --locked -q -- --compact --ascii"
done
