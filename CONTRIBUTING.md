# Contributing

I only have one Razer mouse, so every other model depends on someone who owns it sending a device file. If you have a Razer mouse that isn't in [`devices/`](devices/), please add it, since it's mostly writing a JSONC file and testing it on your own mouse, and send the [protocol file](devices/protocols/) along with it too.

Bug fixes and features are welcome too, just open an issue first if it's something big so we can talk about it, and for a bug in the GUI, attach its log from the journal, which [the debug log](README.md#the-debug-log) shows how to save.

If you want to work on lighting, the polling rate, the battery or presets without a mouse that supports them, [Testing without a mouse](AGENTS.md#testing-without-a-mouse) explains how to fake one.

## Contents

- [Adding a mouse](#adding-a-mouse)
- [Using an agent](#using-an-agent)

## Adding a mouse

A mouse is a single JSONC file, which is JSON with `//` comments, like [the Naga one](devices/razer-naga-left-handed-2020.jsonc), and `vid`, `pid`, `transaction_id` and button ids are hex strings like `"0x40"` so they match what the mouse and the protocol files say.

1. Start the file by running this with the mouse plugged in and opensynapse and its udev rule [installed](README.md#install).

   ```sh
   opensynapse init
   ```

   It only reads from the mouse and writes `~/.config/opensynapse/devices/<vid-pid>/buttons.jsonc`, like `1532-008d`. If you have other Razer devices plugged in, pass the part after the colon in lsusb's `ID 1532:xxxx`, like `opensynapse init 008d`, and if the file already exists, edit it instead of passing `--force`. If you get a permission denied error, the udev rule is missing or you didn't replug the mouse after installing it.

2. Name the buttons with the help of `opensynapse dump`, which shows what each button does today and is usually enough to know which one it is. `opensynapse watch` only shows buttons that send a key, a left/right/middle/back/forward click or a wheel scroll, and buttons with the same action show up together, so use `dump` for the rest. Then rename them in the file and fix the `name`, the `dpi_max`, which is the top DPI on your mouse's Razer product page, and any other `TODO`.

   If `init` can't talk to your mouse, copy the Naga file to `~/.config/opensynapse/devices/<vid-pid>/buttons.jsonc`, set `name`, `pid`, `interface`, `transaction_id`, `profiles`, `layers` and `dpi_max` to your mouse's, delete the "Verified on hardware" line at the top, and replace the buttons with the ids you find with `opensynapse probe`, which is read-only. [PROTOCOL.md](PROTOCOL.md#finding-things-out) and the examples in [devices/protocols/](devices/protocols/README.md) show how to find them.

3. Test it with `opensynapse list` and `opensynapse dump`, and if they print `skipping device definition`, the error gives the line of a JSON typo, so fix it there instead of rerunning init. Then remap one side or thumb button, never left or right click, check it does what you set, and set it back.

   ```sh
   opensynapse export before.json   # `opensynapse import before.json` restores it
   opensynapse set <button> key:z
   opensynapse set <button> <whatever dump showed before>
   ```

4. If you want, add `"at": [x, y]` to the buttons so they show up on the drawn mouse in the GUI instead of in a grid, and the header of [the Naga file](devices/razer-naga-left-handed-2020.jsonc) explains the coordinates.

5. Write the protocol file by running `opensynapse protocol` from the repo root, which only reads from the mouse and writes `devices/protocols/<slug>.md`, where the slug is the `"name"` field in lowercase with dashes, so `Razer Naga Pro (Wired)` is `razer-naga-pro-wired`. Then fill in the TODOs, using [PROTOCOL.md](PROTOCOL.md), which explains what goes in it and has [a prompt so an agent fills it in for you](PROTOCOL.md#using-an-agent), and the [Naga one](devices/protocols/razer-naga-left-handed-2020.md) as a full example.

> If you couldn't make your mouse work, copy the template from PROTOCOL.md, fill in what you found and paste it into a new issue, it's still super useful.

Then send it with `opensynapse protocol send`, which opens an issue here with your protocol and device files titled `New Device: <your mouse>`, and a bot opens the PR that adds your mouse, so once I merge it a patch release goes out.

If you prefer to open a PR yourself, copy `buttons.jsonc` into `devices/` as `<slug>.jsonc`, replace init's two header lines and the `// TODO` notes with a first line like the Naga file's, `// Verified on hardware, firmware vX.Y. See protocols/<slug>.md.`, and open it with both files and a title that starts with `New Device:`, so a release goes out when it's merged. To check it, move the file in `~/.config` away, because it overwrites the one in `devices/`, and run `cargo run -- list` from the repo.

## Using an agent

I did the whole reverse engineering for the Naga with Claude Code, so if you want to do the same for your mouse, open your agent in this repo with the mouse plugged in and paste this prompt.

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
