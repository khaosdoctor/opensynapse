# Contributing

The thing I need the most is mice. I only have one Razer mouse, so every other model depends on someone who owns it sending a device file. If you have a Razer mouse that isn't in [`devices/`](devices/), please add it, it's mostly writing a TOML file and testing it on your own mouse.

Bug fixes and features are welcome too, open an issue first if it's something big so we can talk about it.

## Adding a mouse

A mouse is a single TOML file, like [the Naga one](devices/razer-naga-left-handed-2020.toml).

1. **Start the file.** With the mouse plugged in, run:

   ```sh
   opensynapse init
   ```

   It finds your Razer mouse, looks up its transaction id and interface in a list taken from OpenRGB ([`devices/known-mice.toml`](devices/known-mice.toml)), asks the mouse for its buttons and profiles, and writes `~/.config/opensynapse/devices/<serial>/buttons.toml`, in the same folder where your mouse's profiles are saved. It only reads from the mouse, so it can't change anything. Common buttons get a name, the rest are called `btn-0xNN`.

2. **Name the buttons.** `opensynapse dump` shows what each button does today (a key, a mouse button, DPI up...), which is usually enough to know which one it is, and `opensynapse watch` prints the names as you press them. Rename them in the file, and fix the `dpi_max` and any other `TODO`.

   If `init` can't talk to your mouse, you can do the same by hand with `tools/probe.py`, it only sends read commands: `python3 tools/probe.py /dev/hidrawN 0x02` lists the buttons in the `0x02 id 0x84` line (first byte is the count, then one id per button), and `--args 0140` asks profile 1 what button `0x40` does.

3. **Test it.** Run `opensynapse list` and `opensynapse dump`. If those look right, remap one button, check it does what you set, and set it back:

   ```sh
   opensynapse set <button> key:z
   opensynapse set <button> <whatever dump showed before>
   ```

4. **Optionally, add `at = [x, y]` to the buttons** so they show up on the drawn mouse in the GUI. The body is drawn at x 30..70 and y 2..74 (in percent), anything outside that gets a side plate. If you skip it you get a grid, which is fine.

5. **Write the protocol file (nice to have).** It's not required, but it helps a lot: next to the device file goes `devices/protocols/<same-name>.md`, where you write down what you found and how, and that's what helps the next person decode a similar mouse instead of starting from zero. `opensynapse protocol` generates most of it for you from your device file, you only fill in the TODOs. [PROTOCOL.md](PROTOCOL.md) explains what goes in it and has [a prompt so an agent fills it in for you](PROTOCOL.md#using-an-agent). The [Naga one](devices/protocols/razer-naga-left-handed-2020.md) is a full example. If you couldn't make your mouse work, send the protocol file anyway, what you found is still super useful.

Then send it. The quickest way is `opensynapse protocol send`, which opens an issue here with your protocol and device files (it asks before posting, `--dry-run` shows what it would send). If you prefer a PR, copy `buttons.toml` into `devices/` as `<your-mouse>.toml` and the protocol file into `devices/protocols/`, add the device to the `BUILTIN` list in [`src/device.rs`](src/device.rs) and open it with both files.

## Using an agent

I did the whole reverse engineering for the Naga with Claude Code, and it works really well for this kind of work because it can run the probe, read the answers and try things on the spot. If you want to do the same for your mouse, open your agent of choice in this repo with the mouse plugged in and paste this:

```text
I want to add support for my Razer mouse to opensynapse, the repo you're in.
Read README.md, CONTRIBUTING.md, PROTOCOL.md and
devices/protocols/razer-naga-left-handed-2020.md first, and use
devices/razer-naga-left-handed-2020.toml as the template.

Rules:
- Only send GET commands (command id with bit 0x80 set) while exploring.
  tools/probe.py already does this, prefer it over writing your own.
- Before ANY write to the mouse, tell me exactly which command, button and
  bytes you're about to send, and wait for me to say yes.
- When testing a write, change one button only, read it back to confirm,
  ask me to press it, and then set it back to what it was before.
- Don't touch profiles other than 0 unless I ask.

Steps:
1. Run `opensynapse init` to start the device file in
   ~/.config/opensynapse/devices/<serial>/buttons.toml. If it can't talk to my mouse, find the
   /dev/hidraw node and transaction id by hand with tools/probe.py.
2. Run `opensynapse dump` and work out which id is which physical button
   from what each one does. Ask me to press buttons with
   `opensynapse watch` running if you need to.
3. Rename the buttons, fix dpi_max and every other TODO in the file.
4. Do one write test on a single button following the rules above.
5. Copy the finished file to devices/<my-mouse>.toml.
6. Run `opensynapse protocol` to generate devices/protocols/<my-mouse>.md
   and fill in its TODOs, marking every finding as verified or inferred.
   Don't send it, I'll review it and send it myself.
```

Keep an eye on what it does, it's your mouse.

If you already have a device file and only want the protocol file written, [PROTOCOL.md has a prompt for that](PROTOCOL.md#using-an-agent).
