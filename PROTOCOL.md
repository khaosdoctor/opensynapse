# Writing a protocol file

Every mouse in opensynapse has two files: the device file in [`devices/`](devices/) that the app reads, and a protocol file in [`devices/protocols/`](devices/protocols/) with the same name, where you write down what you found out about the mouse and how. The app never reads the protocol file, it's for the next person (or agent) who needs to know why the device file looks the way it does, or who has a similar mouse and wants a head start.

If you're adding a mouse, write one. The [Naga Left-Handed 2020 file](devices/protocols/razer-naga-left-handed-2020.md) is a complete example.

## Contents

- [What's the same on every Razer mouse](#whats-the-same-on-every-razer-mouse)
- [Finding things out](#finding-things-out)
- [Generating the file](#generating-the-file)
- [Template](#template)
- [Sources](#sources)
- [Using an agent](#using-an-agent)

## What's the same on every Razer mouse

You don't have to repeat this part in your file, only what's different on your mouse.

The mouse talks through a 90-byte HID feature report, usually on USB interface 0, with no report id:

| Offset | Field |
|---|---|
| 0 | status: `02` ok, `01` busy, `03` fail, `05` unsupported |
| 1 | transaction id (`0x1f` on newer mice, older ones use `0xff` or `0x3f`) |
| 5 | data size |
| 6 | command class |
| 7 | command id, bit `0x80` set means it's a read |
| 8..87 | arguments |
| 88 | XOR of bytes 2 to 87 |

You send the report with `HIDIOCSFEATURE` and keep reading it back with `HIDIOCGFEATURE` until the status stops being busy. `tools/probe.py` already does all of this.

## Finding things out

Always start with reads. A read has the `0x80` bit set on the command id, and it doesn't change anything on the mouse, so you can try as many as you want.

`ls -l /sys/class/hidraw/ | grep -i 1532:<pid>` shows which node is which: use the one whose path has `:1.0` (interface 0), or the interface `opensynapse init` printed.

1. **Find the class.** `python3 tools/probe.py /dev/hidrawN` tries every read command in classes `0x00` to `0x0f` and prints the ones that answer. On the Naga the keymap is class `0x02`, DPI is `0x04` and serial/firmware are `0x00`, which seems to be common on newer Razer mice. probe.py sends transaction id `0x1f`. If the `firmware:` line doesn't say `status=ok`, change `tid = 0x1F` in `tools/probe.py` to the `transaction_id` in your device file (`opensynapse init` prints it), or try `0x3F` and `0xFF` if you don't have one.
2. **Look at what comes back with empty arguments.** Some reads answer right away (the Naga's `0x02/0x84` lists every button id), others need arguments.
3. **Retry with arguments.** Some reads only show up once you pass arguments, usually a profile, a button and a layer: `python3 tools/probe.py /dev/hidrawN 0x02 --args 004000` sends profile 0 (the live one, what `opensynapse dump` shows), button `0x40` (use a button id from the `0x84` list instead of `40`), layer 0 to every read in class `0x02`, and the `id 0x8c` line is that button's action.
4. **Decode the answer against what you know.** If a thumb button sends `1` today and the reply ends in `1e`, that's the HID usage for `1`, so you found the key byte.

Only then try a write, with `opensynapse set` once you have a device file, and only on one button: `set` reads it back and errors if it didn't stick, then press the button and set it back to what it was.

**A write that reads back fine doesn't prove anything.** The Naga stores almost any byte you send without checking it, so the only proof that an encoding does what you think is pressing the button and seeing it happen (`opensynapse watch` shows that once you have a device file).

## Generating the file

Once your device file works (`opensynapse dump` shows the right buttons), you don't have to start from an empty file:

```sh
opensynapse protocol
```

That writes `devices/protocols/<slug>.md` (like `razer-naga-left-handed-2020.md`, see [the slug](CONTRIBUTING.md#adding-a-mouse), or `./<slug>.md` when you're not in the repo root), with everything it can know on its own: the USB ids, firmware, interface, transaction id, a table with every button id and what it does on each layer, and the DPI. It only reads from the mouse. Everything only you can know is marked `TODO`, fill those in.

When it's done, send it:

```sh
opensynapse protocol send
```

That opens a GitHub issue on this repo with your protocol file and your device file, using the [GitHub CLI](https://cli.github.com/) as your user (it asks before posting, `--dry-run` shows you what it would send). If you don't have `gh`, it prints a link to open the issue by hand. If you'd rather open a PR, see [CONTRIBUTING.md](CONTRIBUTING.md#adding-a-mouse).

## Template

If you want to write it by hand, copy this into `devices/protocols/<slug>.md` ([slug](CONTRIBUTING.md#adding-a-mouse)) and fill it in. If a section is the same as the Naga's, keep the heading and write "Same as the Naga" under it.

```markdown
# <Mouse name> keymap protocol

Device: USB `1532:xxxx`, firmware vX.Y. Device file: [`<slug>.jsonc`](../<slug>.jsonc).

How you found it: probing on Linux, a USB capture of Synapse, etc.

## Contents

- [Transport](#transport)
- [Keymap commands](#keymap-commands)
- [Button ids](#button-ids)
- [Action encoding](#action-encoding)
- [DPI and other commands](#dpi-and-other-commands)
- [Unknowns](#unknowns)

## Transport

Interface, transaction id, anything that's different from the common frame in PROTOCOL.md.

## Keymap commands

| Command | Args | Status |
|---|---|---|
| `0x02/0x8c` get button | `[profile, button, layer]` | verified / inferred |

## Button ids

| Id | Button |
|---|---|

## Action encoding

| Type | Bytes | Meaning |
|---|---|---|

## DPI and other commands

## Unknowns

What you saw but couldn't explain, and what you didn't try.
```

Mark everything as **verified** (you saw it work on the mouse) or **inferred** (it read back right, or it matched another mouse, but you didn't see it happen). That difference is the most useful thing in the file.

## Sources

Before you probe, check what other people already found, a lot of Razer mice share the same commands:

- [razerqdhid](https://github.com/geezmolycos/razerqdhid) (MIT) reverse engineered the Basilisk V3, which uses the same keymap commands as the Naga. Its docs cover [button actions](https://github.com/geezmolycos/razerqdhid/blob/main/docs/cmd_button-en.md), [macros](https://github.com/geezmolycos/razerqdhid/blob/main/docs/cmd_macro-en.md) and [profiles](https://github.com/geezmolycos/razerqdhid/blob/main/docs/cmd_profile-en.md). Most of the action types in the Naga file came from there.
- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later) has the transaction id, interface and HID usage for almost every Razer device in `Controllers/RazerController/RazerDevices.cpp` and `RazerControllerDetect.cpp`. The transaction ids `opensynapse init` tries (`0x1f`, `0x3f`, `0xff`) come from there.
- [OpenRazer](https://github.com/openrazer/openrazer) (GPL-2.0-or-later) has no button remapping, but its driver picks the transaction id per mouse in `driver/razermouse_driver.c`, and issues [#2845](https://github.com/openrazer/openrazer/issues/2845) (Basilisk Ultimate remapping) and [#2031](https://github.com/openrazer/openrazer/issues/2031) (keyboard remapping) have useful captures.

## Using an agent

You can have an agent (Claude Code or whatever you use) do the probing and write the file. Open it in this repo with the mouse plugged in and paste this:

```text
Write the protocol file for my Razer mouse in opensynapse, the repo you're in.
Read PROTOCOL.md for what goes in it and the template, and
devices/protocols/razer-naga-left-handed-2020.md as an example.
My device file is ~/.config/opensynapse/devices/<vid-pid>/buttons.jsonc
(from `opensynapse init`), or the mouse is built in under devices/. If
neither exists, say so and stop.

Rules:
- Only send GET commands (command id with bit 0x80 set), use tools/probe.py.
  Don't write anything to the mouse.
- Only write down what you actually saw. Mark each finding as verified
  (seen working on the mouse) or inferred (read back fine, or matches the
  Naga, but not seen working). Anything you couldn't explain goes under
  Unknowns.

Steps:
1. Find the /dev/hidraw node for the interface in my device file
   (`ls -l /sys/class/hidraw/`).
2. Run tools/probe.py over classes 0x00 to 0x0f and note which reads answer.
3. Read the button list (0x02/0x84), then run tools/probe.py on class 0x02
   with --args <profile><button><layer> and read the id 0x8c line for what
   each button does. Compare with my device file.
4. Ask me to press buttons while `timeout 20 opensynapse watch` runs if you
   need to confirm which id is which button.
5. Run `opensynapse protocol` to generate devices/protocols/<slug>.md
   and fill in every TODO with what you found. If that file already
   exists, edit it instead and never pass --force. Don't send it, I'll review
   it and run `opensynapse protocol send` myself.
```

Read what it wrote before you send it, agents like to sound more sure than they are.
