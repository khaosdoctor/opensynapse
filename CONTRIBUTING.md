# Contributing

I only have one Razer mouse, so every other model depends on someone who owns it sending a device file. If you have a Razer mouse that isn't in [`devices/`](devices/), please add it, it's mostly writing a JSONC file and testing it on your own mouse.

Bug fixes and features are welcome too, open an issue first if it's something big so we can talk about it.

## Contents

- [Adding a mouse](#adding-a-mouse)
- [Using an agent](#using-an-agent)

## Adding a mouse

A mouse is a single JSONC file (JSON with `//` comments), like [the Naga one](devices/razer-naga-left-handed-2020.jsonc). `vid`, `pid`, `transaction_id` and button ids are hex strings (`"0x40"`) so they match what the mouse and the protocol files say.

1. **Start the file.** With the mouse plugged in, and opensynapse and its udev rule installed as in the README's [Install](README.md#install) section, run:

   ```sh
   opensynapse init
   ```

   It finds a Razer mouse that has no device file yet and writes `~/.config/opensynapse/devices/<vid-pid>/buttons.jsonc` (like `1532-008d`). It only reads from the mouse, so it can't change anything. Common buttons get a name, the rest are called `btn-0xNN`, and everything you need to check has a `// TODO` next to it. If you have other Razer devices plugged in, pass the part after the colon in lsusb's `ID 1532:xxxx` (`opensynapse init 008d`). If the file already exists, `init` stops and asks for `--force`, edit the file instead. If it says permission denied, the udev rule isn't installed yet, or you need to unplug and replug the mouse after installing it.

2. **Name the buttons.** `opensynapse dump` shows what each button does today (a key, a mouse button, DPI up...), which is usually enough to know which one it is. `opensynapse watch` prints a button's name when you press it. Only buttons that send a key, a left/right/middle/back/forward click or a wheel scroll show up (not tilt, DPI, profile, media, hypershift or disabled), and buttons with the same action show up together, so use `dump` for those. Rename them in the file, and fix the `name`, the `dpi_max` (the top DPI on your mouse's Razer product page) and any other `TODO`.

   If `init` can't talk to your mouse, copy the Naga file to `~/.config/opensynapse/devices/<vid-pid>/buttons.jsonc`, set `name`, `pid`, `interface`, `transaction_id`, `profiles`, `layers` and `dpi_max` to your mouse's, delete the "Verified on hardware" line at the top, and replace the buttons with the ids you find with `opensynapse probe` (read-only, it finds the mouse itself when it's the only Razer device plugged in), see [PROTOCOL.md](PROTOCOL.md#finding-things-out) and the examples in [devices/protocols/](devices/protocols/README.md).

3. **Test it.** Run `opensynapse list` and `opensynapse dump`. If they print `skipping device definition`, the error gives the line of a JSON typo (usually a missing comma or quote), fix it there and don't rerun init. If those look right, remap one side or thumb button (not left or right click), check it does what you set, and set it back:

   ```sh
   opensynapse export before.json   # backup, `opensynapse import before.json` puts it back
   opensynapse set <button> key:z
   opensynapse set <button> <whatever dump showed before>
   ```

4. **Optionally, add `"at": [x, y]` to the buttons** so they show up on the drawn mouse in the GUI, the header of [the Naga file](devices/razer-naga-left-handed-2020.jsonc) explains the coordinates. If you skip it you get a grid, which is fine.

5. **Write the protocol file.** `opensynapse protocol`, run from the repo root, writes `devices/protocols/<slug>.md` (the `"name"` field in lowercase with dashes, so `Razer Naga Pro (Wired)` is `razer-naga-pro-wired`). It needs the device file from the earlier steps and reads from the mouse, it never writes to it. You only need to fill in the TODOs. The [PROTOCOL.md](PROTOCOL.md) explains what goes in it and has [a prompt so an agent fills it in for you](PROTOCOL.md#using-an-agent), if you can, add one because it would greatly help to understand the protocols for each mouse, but it's an optional nice to have. The [Naga one](devices/protocols/razer-naga-left-handed-2020.md) is a full example.

> If you couldn't make your mouse work, copy the template from PROTOCOL.md, fill in what you found and paste it into a new issue, it's still super useful.

Then send it. The quickest way is `opensynapse protocol send`, which opens an issue here with your protocol and device files. You don't need to do anything else after that. Once I've had a look I label it `add-device` and a bot opens the PR that adds your mouse. As soon as that PR is merged (it carries the `new-device` label) a patch release goes out with it, so it reaches the AUR and the other packages within the hour. If you prefer a PR yourself, copy `buttons.jsonc` into `devices/` as `<slug>.jsonc`, replace init's two header lines and the `// TODO` notes with a first line like the Naga file's (`// Verified on hardware, firmware vX.Y. See protocols/<slug>.md.`), and open it with both files. Start the PR title with `New Device:`, like `New Device: Razer Naga X`, and a bot labels it so a release goes out when it's merged. That's all the code needs, the app reads `devices/*.jsonc` when it starts. To check it, move the file in `~/.config` away, because it wins over the one in `devices/`, and run `cargo run -- list` from the repo. Your mouse should still be there.

## Using an agent

I did the whole reverse engineering for the Naga with Claude Code because, well... I haven't learned to read syscodes yet. 

It works really well for this kind of work because it can run the probe, read the answers and try things faster than you would. If you want to do the same for your mouse, open your agent of choice in this repo with the mouse plugged in and paste this:

```text
I want to add support for my Razer mouse to opensynapse, the repo you're in.
Read README.md, CONTRIBUTING.md, PROTOCOL.md and
devices/protocols/razer-naga-left-handed-2020.md first, and use
devices/razer-naga-left-handed-2020.jsonc as an example of a finished
device file.

Rules:
- Only send GET commands (command id with bit 0x80 set) while exploring.
  Use `opensynapse probe` for this, don't write your own HID code.
- Before ANY write to the mouse, tell me exactly which command, button and
  bytes you're about to send, and wait for me to say yes.
- When testing a write, change one button only, read it back to confirm,
  ask me to press it, and then set it back to what it was before.
- Don't touch profiles other than 0 unless I ask.
- `opensynapse set`, `dpi <X>`, `stages` with DPI values or `--active`, `rgb`, `import` and the
  GUI write to the mouse too. `list`, `dump`, `get`, `export`, `watch`, `init`, `probe` and
  `protocol` (without `send`) only read; `protocol send` posts a public issue, never run it.
- Never pass --force. If `init` or `protocol` says the file already exists, edit that file instead.

Steps:
1. Run `opensynapse init` to start the device file in
   ~/.config/opensynapse/devices/<vid-pid>/buttons.jsonc. If it can't talk to my mouse, find the
   transaction id and interface by hand with `opensynapse probe`. It finds the mouse itself when
   it's the only Razer device plugged in, otherwise pass it the /dev/hidraw node it lists.
2. Run `opensynapse dump` and work out which id is which physical button
   from what each one does. Ask me to press buttons while
   `timeout 20 opensynapse watch` runs if you need to.
3. Rename the buttons and fix every TODO in the file. Ask me for dpi_max
   (the top DPI on my mouse's Razer product page), don't guess it.
4. Run `opensynapse export before.json` first, then do one write test on a
   single button following the rules above. If anything goes wrong, ask me
   before running `opensynapse import before.json`.
5. Run `opensynapse protocol` to generate devices/protocols/<slug>.md
   and fill in its TODOs, marking every finding as verified or inferred.
   Don't send it, I'll review it and send it myself.
```

Thanks for contributing!
