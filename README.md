<p align="center"><img src="assets/icon.svg" width="128" alt="opensynapse icon"></p>

# opensynapse

> Remap buttons, set DPI and lighting on Razer mice from Linux.

> [!WARNING]
> **This is a work in progress.** I tested it on my mouse only

My lawyers say I have to write this so:

> Not affiliated with, endorsed by, or supported by Razer Inc. Everything here is distributed as-is and with no warranty.

## Contents

- [What is it](#what-is-it)
- [Current status](#current-status)
- [Install](#install)
- [First use](#first-use)
  - [In the GUI](#in-the-gui)
  - [With the CLI](#with-the-cli)
- [Usage](#usage)
- [Adding your mouse](#adding-your-mouse)
- [AI usage](#ai-usage)

## What is it

Razer doesn't ship Synapse for Linux, to change the keybinds we can use keyd and the likes of it so we can remap what the mouse sends to the computer, but the remap sometimes miss buttons, some buttons are weird stuff (like keys that don't really exist) and the final remaps stay on the computer, not on the mouse. I also happen to have a Razer mice, and a Linux computer, so that was _pretty_ annoying... For... Years...

This is the reverse engineered version of the Synapse protocol handling. It maps out how the mouse stores its keymap (the details are in [`devices/protocols/`](devices/protocols/razer-naga-left-handed-2020.md)). It talks to the mouse directly, so the mapping is saved on the mouse itself and works in any computer, this is a single Rust binary either so you can probably build for whatever OS you have.

Right now you can:

- Remap any button to a key (with modifiers), a mouse button, a media key, DPI up/down or a DPI stage, switching profiles, Hypershift or disable it.
- Normal and the Hypershift layers.
- Keys and mouse buttons can also be on turbo, so they repeat while you hold them
- Change DPI and the DPI stages
- Export and import profiles as JSON
- See which button you're pressing, live if you have the GUI, which becomes some sort of tester
- Change the lighting and brightness of zones through [OpenRazer](https://openrazer.github.io/) (install `openrazer-driver-dkms` on Arch), so it works on any mouse OpenRazer supports.

![The buttons tab: the Naga drawn with what each button does, the DPI stages and the remap editor](docs/screenshots/buttons.png)

![The lighting tab: effect and brightness for each zone](docs/screenshots/lighting.png)

## Current status

Only the **Razer Naga Left-Handed Edition 2020** (`1532:008d`) is tested and supported, because that's the one I have. The protocol is shared by a bunch of Razer mice, so adding another one should mostly be writing a JSONC file (see [Adding your mouse](#adding-your-mouse)).

## Install

On Arch, it's on the AUR. The package also installs the udev rule, so you can skip the rest of this section:

```sh
yay -S opensynapse      # builds the latest release
yay -S opensynapse-bin  # same release, prebuilt
yay -S opensynapse-git  # builds the latest main
```

On Debian and Ubuntu, add the apt repository. The package also installs the udev rule:

```sh
curl -fsSL https://oss.lsantos.dev/opensynapse/apt/public.key | sudo gpg --dearmor -o /usr/share/keyrings/opensynapse.gpg
echo "deb [signed-by=/usr/share/keyrings/opensynapse.gpg arch=$(dpkg --print-architecture)] https://oss.lsantos.dev/opensynapse/apt stable main" | sudo tee /etc/apt/sources.list.d/opensynapse.list
sudo apt update && sudo apt install opensynapse
```

With Homebrew on Linux (then install the udev rule as `brew` tells you):

```sh
brew install khaosdoctor/tap/opensynapse
```

Every [release](https://github.com/khaosdoctor/opensynapse/releases) also has a `.deb`, an AppImage (GUI only) and a tar.gz with both binaries, for x86_64 and arm64. The `.deb` installs the udev rule; with the other two, install it as shown at the end of this section.

To build it yourself, you need [Rust](https://rustup.rs) and a Linux machine. Then:

```sh
git clone https://github.com/khaosdoctor/opensynapse && cd opensynapse
cargo install --path . --features gui
```

That puts `opensynapse` (the CLI) and `opensynapse-gui` in `~/.cargo/bin`. If you only want the CLI, drop the `--features gui`.

Your user also needs access to the mouse's raw HID device, otherwise you'll get a permission denied:

```sh
sudo install -m644 udev/70-opensynapse.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
```

## First use

Plug the mouse in. If you just installed the udev rule, unplug it and plug it back once so it gets the new permissions. Then check that opensynapse sees it:

```sh
opensynapse list
```

If it says permission denied, the udev rule isn't installed yet or the mouse wasn't replugged. If your mouse isn't in the list, it isn't supported yet, see [Adding your mouse](#adding-your-mouse).

### In the GUI

Open `opensynapse-gui`, or OpenSynapse from your app launcher. The first time it sees your mouse it saves every profile and the lighting as the original, so ***RESET*** always has something to go back to. Then:

1. Pick ***P1*** under PROFILE, on the left of the mouse drawing. On the Naga, P0 is the live profile and is lost when you unplug the mouse, P1 is the one it keeps
2. Click a button on the drawing and pick what it should do in the editor below. Every change goes to ***PENDING*** in the sidebar
3. Hit ***WRITE TO DEVICE*** and try the button

The lighting tab needs the OpenRazer daemon running, the badge at the top says ***OPENRAZER ONLINE*** when it is.

### With the CLI

Make a backup first, then change one button and check it:

```sh
opensynapse export before.json --profile 1
opensynapse set thumb1 key:f13 --profile 1
opensynapse get thumb1 --profile 1
```

If you don't like it, this puts everything back:

```sh
opensynapse import before.json --profile 1
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
opensynapse rgb all static ff00aa        # lighting, needs OpenRazer
opensynapse rgb scroll brightness 128
opensynapse init                         # start a device file for an unsupported mouse
opensynapse protocol                     # protocol file for your mouse, see PROTOCOL.md
opensynapse protocol send                # share it as a GitHub issue
```

`opensynapse help` has the rest. CLI commands use profile 0 unless you pass `--profile`, and on the Naga that's the live profile that's lost when you unplug the mouse, so add `--profile 1` for changes you want to keep.

Every package installs shell completions for bash, zsh and fish, which also complete your mouse's button names. If you built it yourself, they're in [`contrib/completions/`](contrib/completions/).

## Adding your mouse

**I need your help here.** I only have one Razer mouse, so if you have another one, please add it. Every mouse is a single JSONC file (JSON with comments) with its USB ids and the list of buttons. 

I made `opensynapse init` to write most of it for you to `~/.config/opensynapse/devices/<vid-pid>/buttons.jsonc` and making the process simpler.

[CONTRIBUTING.md](CONTRIBUTING.md) has the step by step, and a prompt you can paste into Claude Code (or whatever agent you use) so it does it for you.

## AI usage

I used AI a lot on this, especially for the reverse engineering part, and I checked everything against the real mouse, but there is probably something I missed. If you find it, open an [issue](https://github.com/khaosdoctor/opensynapse/issues).
