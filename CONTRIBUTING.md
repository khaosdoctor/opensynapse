# Contributing

I only have one Razer mouse, so every other model depends on someone who owns it sending a device file. If you have a Razer mouse that isn't in [`devices/`](devices/), please add it, it's mostly writing a JSONC file and testing it on your own mouse.

Please send the [protocol file](devices/protocols/) along with it too.

Bug fixes and features are welcome too, open an issue first if it's something big so we can talk about it. For a bug in the GUI, attach its log from the journal, see [the debug log](README.md#the-debug-log) for the command.

To work on lighting, the polling rate, the battery or presets without a mouse that supports them, see [Testing without a mouse](AGENTS.md#testing-without-a-mouse).

## Contents

- [Adding a mouse](#adding-a-mouse)
- [Using an agent](#using-an-agent)

## Adding a mouse

A mouse is a single JSONC file (JSON with `//` comments), like [the Naga one](devices/razer-naga-left-handed-2020.jsonc). `vid`, `pid`, `transaction_id` and button ids are hex strings (`"0x40"`) so they match what the mouse and the protocol files say.

1. **Start the file.** With the mouse plugged in, and opensynapse and its udev rule [installed](README.md#install), run this.

   ```sh
   opensynapse init
   ```

   It writes `~/.config/opensynapse/devices/<vid-pid>/buttons.jsonc` (like `1532-008d`) and only reads from the mouse. If you have other Razer devices plugged in, pass the part after the colon in lsusb's `ID 1532:xxxx`, like `opensynapse init 008d`. If the file already exists, edit it instead of passing `--force`. Permission denied means the udev rule is missing or the mouse wasn't replugged after installing it.

2. **Name the buttons.** `opensynapse dump` shows what each button does today, which is usually enough to know which one it is. `opensynapse watch` only shows buttons that send a key, a left/right/middle/back/forward click or a wheel scroll, and buttons with the same action show up together, so use `dump` for the rest. Rename them in the file, and fix the `name`, the `dpi_max` (the top DPI on your mouse's Razer product page) and any other `TODO`.

   If `init` can't talk to your mouse, copy the Naga file to `~/.config/opensynapse/devices/<vid-pid>/buttons.jsonc`, set `name`, `pid`, `interface`, `transaction_id`, `profiles`, `layers` and `dpi_max` to your mouse's, delete the "Verified on hardware" line at the top, and replace the buttons with the ids you find with `opensynapse probe`, which is read-only. See [PROTOCOL.md](PROTOCOL.md#finding-things-out) and the examples in [devices/protocols/](devices/protocols/README.md).

3. **Test it.** Run `opensynapse list` and `opensynapse dump`. If they print `skipping device definition`, the error gives the line of a JSON typo, fix it there and don't rerun init. Then remap one side or thumb button (not left or right click), check it does what you set, and set it back.

   ```sh
   opensynapse export before.json   # `opensynapse import before.json` restores it
   opensynapse set <button> key:z
   opensynapse set <button> <whatever dump showed before>
   ```

4. **Optionally, add `"at": [x, y]` to the buttons** so they show up on the drawn mouse in the GUI, the header of [the Naga file](devices/razer-naga-left-handed-2020.jsonc) explains the coordinates. Without it you get a grid.

5. **Write the protocol file.** `opensynapse protocol`, run from the repo root, writes `devices/protocols/<slug>.md`. The slug is the `"name"` field in lowercase with dashes, so `Razer Naga Pro (Wired)` is `razer-naga-pro-wired`. It only reads from the mouse. Fill in the TODOs. [PROTOCOL.md](PROTOCOL.md) explains what goes in it and has [a prompt so an agent fills it in for you](PROTOCOL.md#using-an-agent). The [Naga one](devices/protocols/razer-naga-left-handed-2020.md) is a full example.

> If you couldn't make your mouse work, copy the template from PROTOCOL.md, fill in what you found and paste it into a new issue, it's still super useful.

Then send it. `opensynapse protocol send` opens an issue here with your protocol and device files, titled `New Device: <your mouse>`. A bot opens the PR that adds your mouse, and once I merge it a patch release goes out.

If you prefer a PR yourself, copy `buttons.jsonc` into `devices/` as `<slug>.jsonc`, replace init's two header lines and the `// TODO` notes with a first line like the Naga file's (`// Verified on hardware, firmware vX.Y. See protocols/<slug>.md.`), and open it with both files. Start the PR title with `New Device:` so a release goes out when it's merged. To check it, move the file in `~/.config` away, because it wins over the one in `devices/`, and run `cargo run -- list` from the repo.

## Using an agent

I did the whole reverse engineering for the Naga with Claude Code. To do the same for your mouse, open your agent in this repo with the mouse plugged in and paste this.

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
- `opensynapse set`, `dpi <X>`, `stages` with DPI values or `--active`, `rgb`, `import`,
  `profile create|delete|switch` and the GUI write to the mouse too. `list`, `dump`, `get`,
  `export`, `watch`, `init`, `probe`, `profile` and `protocol` (without `send`) only read; `protocol send` posts a public issue, never run it.
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
