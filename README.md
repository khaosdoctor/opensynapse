# opensynapse

> Remap buttons and set DPI on Razer mice from Linux.

> [!WARNING]
> **This is a work in progress.** It works on my mouse and that's about it for now. Expect things to change, break and get renamed.

My lawyers say I have to write this so:

> Not affiliated with, endorsed by, or supported by Razer Inc. Everything here is distributed as-is and with no warranty.

## What is it

Razer doesn't ship Synapse for Linux, and my Naga's thumb grid was basically useless here. keyd and friends can remap what the mouse sends, but the remap lives on the computer, not on the mouse, and some things you just can't reach that way.

So I reverse engineered how the mouse stores its keymap (it's all HID feature reports, the details are in [PROTOCOL.md](PROTOCOL.md)) and wrote this. It talks to the mouse directly, so the mapping is saved on the mouse itself and goes with it to any computer.

Right now you can:

- Remap any button to a key (with modifiers), a mouse button, a mouse button on turbo, DPI up/down or nothing at all, on both the normal and the Hypershift layer
- Change DPI and the DPI stages
- Export and import profiles as JSON. The first time it runs it backs up every profile on the mouse to `~/.config/opensynapse/devices/<serial>/`
- See which button you're pressing, live, in the GUI

Changes are queued like in GParted, nothing touches the mouse until you hit _Write to device_.

## Current status

Only the **Razer Naga Left-Handed Edition 2020** (`1532:008d`) is supported, because that's the one I have. The protocol is shared by a bunch of Razer mice, so adding another one should mostly be writing a TOML file (see [Adding your mouse](#adding-your-mouse)).

Things I still haven't figured out: turbo for keyboard keys, media keys like play/pause, and whether profile 0 survives unplugging the mouse. RGB is not here at all, use [OpenRazer](https://openrazer.github.io/) for that.

## Install

You need Rust and a Linux box. Then:

```sh
cargo build --release --features gui
```

That gives you two binaries in `target/release`, `opensynapse` (the CLI) and `opensynapse-gui`. If you only want the CLI, drop the `--features gui`.

Your user also needs access to the mouse's raw HID device, otherwise you'll get a permission denied:

```sh
sudo install -m644 udev/70-opensynapse.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
```

## Usage

The GUI is the easy way, just run `opensynapse-gui`. For headless use there's the CLI:

```sh
opensynapse dump                         # every button and what it does
opensynapse set thumb1 key:ctrl+1        # remap a button
opensynapse set thumb2 key:f13 --layer hypershift
opensynapse dpi 1600
opensynapse stages 800 1600 3200 --active 2
opensynapse export my-profile.json
opensynapse watch                        # prints buttons as you press them
```

`opensynapse help` has the rest.

## Adding your mouse

Every mouse is a TOML file in [`devices/`](devices/). It has the USB ids, the profiles, the layers and the list of buttons with their ids. Each button can also have an optional `at = [x, y]` so it shows up on the drawn mouse in the GUI, if you skip that you get a grid instead.

To find the button ids of your mouse, `tools/probe.py` asks the mouse for them without writing anything. You can also drop your file in `~/.config/opensynapse/devices/` to try it before opening a PR.

## AI usage

I used AI a lot on this, including for the reverse engineering part, and I checked everything against the real mouse, but there is probably something I missed. If you find it, open an [issue](https://github.com/khaosdoctor/opensynapse/issues).
