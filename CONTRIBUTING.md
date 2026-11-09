# Contributing

The thing I need the most is mice. I only have one Razer mouse, so every other model depends on someone who owns it sending a device file. If you have a Razer mouse that isn't in [`devices/`](devices/), please add it, it's mostly writing a TOML file and testing it on your own mouse.

Bug fixes and features are welcome too, open an issue first if it's something big so we can talk about it.

## Adding a mouse

A mouse is a single TOML file. The best way to start is copying [the Naga one](devices/razer-naga-left-handed-2020.toml) and changing it. You need to find out a few things:

1. **The USB ids.** Run `lsusb | grep 1532`, Razer is `1532` and the part after the colon is your mouse's product id.
2. **Which hidraw node talks to the mouse.** Most Razer mice use interface 0. `ls -l /dev/input/by-id/ | grep -i razer` shows the hidraw nodes, the one without `if0X` in the name is usually interface 0.
3. **The button ids.** Run the probe, it only sends read commands so it can't change anything on your mouse:

   ```sh
   python3 tools/probe.py /dev/hidrawN 0x02
   ```

   The line for `0x02 id 0x84` lists the buttons: the first byte is how many there are, then one id per button.

4. **Which id is which physical button.** Ask the mouse what each id currently does:

   ```sh
   python3 tools/probe.py /dev/hidrawN 0x02 --args 0140
   ```

   That asks profile 1 for button `0x40` (change the second byte for each id). The reply tells you what it does today (a key, a mouse button, DPI up...), which is usually enough to know which button it is. Once the file exists, `opensynapse watch` prints the button names as you press them, so you can double check.

5. **Put the file in `~/.config/opensynapse/devices/`** and run `opensynapse list` and `opensynapse dump`. If those look right, remap one button, check it does what you set, and set it back:

   ```sh
   opensynapse set <button> key:z
   opensynapse set <button> <whatever dump showed before>
   ```

6. **Optionally, add `at = [x, y]` to the buttons** so they show up on the drawn mouse in the GUI. The body is drawn at x 30..70 and y 2..74 (in percent), anything outside that gets a side plate. If you skip it you get a grid, which is fine.

7. **Write the protocol file (nice to have).** It's not required, but it helps a lot: next to the device file goes `devices/protocols/<same-name>.md`, where you write down what you found and how, and that's what helps the next person decode a similar mouse instead of starting from zero. `opensynapse protocol` generates most of it for you from your device file, you only fill in the TODOs. [PROTOCOL.md](PROTOCOL.md) explains what goes in it and has [a prompt so an agent fills it in for you](PROTOCOL.md#using-an-agent). The [Naga one](devices/protocols/razer-naga-left-handed-2020.md) is a full example. If you couldn't make your mouse work, send the protocol file anyway, what you found is still super useful.

Then send it. The quickest way is `opensynapse protocol send`, which opens an issue here with your protocol and device files (it asks before posting, `--dry-run` shows what it would send). If you prefer a PR, copy the device file into `devices/` and the protocol file into `devices/protocols/`, add the device to the `BUILTIN` list in [`src/device.rs`](src/device.rs) and open it with both files.

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
1. Find my mouse with lsusb and figure out which /dev/hidraw node is
   interface 0.
2. Run tools/probe.py on class 0x02 to get the button list (0x02/0x84),
   then read what every button currently does (0x02/0x8c with
   [profile, button, layer]).
3. Work out which id is which physical button from those answers. Ask me
   to press buttons if you need to.
4. Write devices/<my-mouse>.toml, put a copy in
   ~/.config/opensynapse/devices/ and check `opensynapse dump` matches
   what you read.
5. Do one write test on a single button following the rules above.
6. Run `opensynapse protocol` to generate devices/protocols/<my-mouse>.md
   and fill in its TODOs, marking every finding as verified or inferred.
   Don't send it, I'll review it and send it myself.
```

Keep an eye on what it does, it's your mouse.

If you already have a device file and only want the protocol file written, [PROTOCOL.md has a prompt for that](PROTOCOL.md#using-an-agent).
