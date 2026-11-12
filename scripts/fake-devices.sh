#!/usr/bin/env bash
# Creates OpenRazer fake devices and runs a daemon for them on a private
# session bus, so you can try the lighting, polling rate, battery, sleep
# timeout and presets of a debug build without a mouse. The daemon on your
# normal session bus is never touched.
#
# usage: scripts/fake-devices.sh [model ...] [-- command ...]
#        scripts/fake-devices.sh --list
#
# Without a command you get a shell, exit it to stop everything. The fake
# device generator only ships in OpenRazer's source, so the first run downloads
# the release that matches your openrazer-daemon to ~/.cache/opensynapse.
set -euo pipefail

die() { echo "fake-devices: $*" >&2; exit 1; }

command -v openrazer-daemon >/dev/null || die "openrazer-daemon isn't installed"
command -v dbus-run-session >/dev/null || die "dbus-run-session isn't installed"
command -v python3 >/dev/null || die "python3 isn't installed"

ver=$(openrazer-daemon --version | awk '{print $NF}')
[[ $ver =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "can't read the openrazer-daemon version: $ver"
src="${XDG_CACHE_HOME:-$HOME/.cache}/opensynapse/openrazer-$ver"
if [[ ! -d $src ]]; then
  echo "downloading OpenRazer $ver to $src"
  mkdir -p "$(dirname "$src")"
  part=$(mktemp -d "$src.XXXXXX")
  curl -fsSL "https://github.com/openrazer/openrazer/archive/refs/tags/v$ver.tar.gz" | tar xz -C "$part" --strip-components=1 \
    || { rm -rf "$part"; die "download failed"; }
  mv "$part" "$src"
fi
cfgs="$src/pylib/openrazer/_fake_driver"

if [[ ${1:-} == --list ]]; then
  for cfg in "$cfgs"/*.cfg; do
    printf '%-50s %s\n' "$(basename "$cfg" .cfg)" "$(sed -n 's/^name = //p' "$cfg")"
  done
  exit 0
fi

models=()
while (($#)) && [[ $1 != -- ]]; do
  models+=("$1")
  shift
done
[[ ${1:-} == -- ]] && shift
# a wired mouse that has a device file, and a wireless one with a battery and a sleep timeout
((${#models[@]})) || models=(razernagaleft-handededition2020 razerviperultimatewireless)
for m in "${models[@]}"; do
  [[ -f $cfgs/$m.cfg ]] || die "no fake device called $m, see --list"
done

tmp=$(mktemp -d "${TMPDIR:-/tmp}/opensynapse-fake.XXXXXX")
export OPENSYNAPSE_OPENRAZER_TEST="$tmp/devices"
fake_pid=
cleanup() {
  if [[ -n $fake_pid ]]; then
    kill "$fake_pid" 2>/dev/null || true
    wait "$fake_pid" 2>/dev/null || true
  fi
  # the generator makes each device folder read-only
  chmod -R u+w "$tmp"
  rm -rf "$tmp"
}
trap cleanup EXIT

echo "creating ${models[*]}"
# it keeps running because it holds the devices' input pipes open
python3 -u "$src/scripts/create_fake_device.py" --dest "$OPENSYNAPSE_OPENRAZER_TEST" --non-interactive "${models[@]}" >"$tmp/generator.log" 2>&1 &
fake_pid=$!
for _ in {1..50}; do
  grep -q '^Sleeping forever' "$tmp/generator.log" && break
  kill -0 "$fake_pid" 2>/dev/null || { cat "$tmp/generator.log" >&2; die "the generator stopped"; }
  sleep 0.1
done

# without service folders nothing can start the real daemon on this bus
printf '%s\n' '<busconfig>' '<type>session</type>' '<listen>unix:tmpdir=/tmp</listen>' '<auth>EXTERNAL</auth>' \
  '<policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy>' \
  '</busconfig>' >"$tmp/bus.conf"

inside_bus() {
  openrazer-daemon -F --test-dir "$OPENSYNAPSE_OPENRAZER_TEST" --run-dir "$tmp/daemon" --log-dir "$tmp/daemon" \
    --config "$tmp/daemon/razer.conf" --persistence "$OPENSYNAPSE_OPENRAZER_TEST/config/openrazer/persistence.conf" \
    >"$tmp/daemon.log" 2>&1 &
  daemon_pid=$!
  trap 'kill $daemon_pid 2>/dev/null; wait $daemon_pid 2>/dev/null' EXIT
  for _ in {1..100}; do
    dbus-send --session --print-reply --dest=org.razer /org/razer razer.devices.getDevices >/dev/null 2>&1 && break
    kill -0 "$daemon_pid" 2>/dev/null || break
    sleep 0.1
  done
  if ! dbus-send --session --print-reply --dest=org.razer /org/razer razer.devices.getDevices; then
    cat "$tmp/daemon.log" >&2
    echo "fake-devices: the daemon didn't start" >&2
    return 1
  fi
  echo "OPENSYNAPSE_OPENRAZER_TEST=$OPENSYNAPSE_OPENRAZER_TEST"
}
export -f inside_bus
export tmp

if (($#)); then
  dbus-run-session --config-file="$tmp/bus.conf" -- bash -c 'inside_bus && "$@"' _ "$@"
  exit
fi
dbus-run-session --config-file="$tmp/bus.conf" -- bash -c 'inside_bus && echo "exit this shell to stop them" && "${SHELL:-bash}"'
