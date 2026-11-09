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

7. **Write the protocol file.** Next to the device file goes `devices/protocols/<same-name>.md`, where you write down what you found and how, so the next person with a similar mouse doesn't start from zero. [PROTOCOL.md](PROTOCOL.md) explains what goes in it and has a template, and the [Naga one](devices/protocols/razer-naga-left-handed-2020.md) is a full example. If you couldn't make your mouse work, send the protocol file anyway, what you found is still super useful.

Then copy the device file into `devices/`, add it to the `BUILTIN` list in [`src/device.rs`](src/device.rs) and open a PR with both files. Mention the firmware version (`opensynapse export` writes it to the profile file).

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
6. Write devices/protocols/<my-mouse>.md following the template in
   PROTOCOL.md, marking every finding as verified or inferred.
```

Keep an eye on what it does, it's your mouse.

If you already have a device file (or you did the probing by hand) and only want the protocol file written, paste this instead:

```text
Write the protocol file for my Razer mouse in opensynapse, the repo you're in.
Read PROTOCOL.md for what goes in it and the template, and
devices/protocols/razer-naga-left-handed-2020.md as an example.
My device file is devices/<my-mouse>.toml.

Rules:
- Only send GET commands (command id with bit 0x80 set), use tools/probe.py.
  Don't write anything to the mouse.
- Only write down what you actually saw. Mark each finding as verified
  (seen working on the mouse) or inferred (read back fine, or matches the
  Naga, but not seen working). Anything you couldn't explain goes under
  Unknowns.
- Only document what's different from the common frame in PROTOCOL.md.

Steps:
1. Find my mouse with lsusb and the /dev/hidraw node for its interface.
2. Run tools/probe.py over classes 0x00 to 0x0f and note which reads answer.
3. Read the button list (0x02/0x84) and what each button does (0x02/0x8c
   with [profile, button, layer]) and compare with my device file.
4. Ask me to press buttons with `opensynapse watch` running if you need to
   confirm which id is which button.
5. Write devices/protocols/<my-mouse>.md with the same name as the
   device file.
```
