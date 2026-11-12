# Razer Naga Left-Handed Edition 2020 keymap protocol

This is the USB device `1532:008d` on firmware v1.0, which Synapse 4 calls device 141, edition 0, or `Nagalhe_UI`, and its device file is [`razer-naga-left-handed-2020.jsonc`](../razer-naga-left-handed-2020.jsonc).

Everything here comes from talking to the mouse on Linux through `/dev/hidraw` on interface 0, without ever running Synapse, and you can't get the packet encoder out of Synapse anyway, since its shipped UI, installer and driver package don't contain it.

## Contents

- [Transport](#transport)
- [Keymap commands (class `0x02`)](#keymap-commands-class-0x02)
- [Button ids](#button-ids)
- [Action encoding](#action-encoding)
- [Lighting (class `0x0f`)](#lighting-class-0x0f)
- [Profiles (class `0x05`)](#profiles-class-0x05)
- [Unknowns](#unknowns)

## Transport

It uses the common frame from [PROTOCOL.md](../../PROTOCOL.md#the-frame-every-razer-mouse-shares), on interface 0 with transaction id `0x1f`.

## Keymap commands (class `0x02`)

| Command | Args | Status |
|---|---|---|
| `0x8c` get button | `[profile, button, layer]` | the reply echoes the args and then the action, verified |
| `0x0c` set button | `[profile, button, layer, action...]`, size `0x0a` | verified, with status ok, a matching read-back and the keypress seen |
| `0x84` list buttons | none | `count` + button ids |
| `0x8e` bulk dump | `[profile, button, layer]` | 8-byte entries, layout not decoded |
| `0x82` | none | `08 1c 08 1c`, meaning unknown |

Layer `0` is the normal layer and `1` is Hypershift, and you can read profiles `0`..`5`. Profile `0` is the live profile, so if you write to it the behaviour changes right away, profile `1` stays untouched and the change is gone on a replug, while writing profile `1` when it's the active profile also changes the behaviour right away but survives a replug. You can even write profiles `2`..`5` before they exist, which [Profiles](#profiles-class-0x05) explains.

## Button ids

| Id | Button |
|---|---|
| `01` / `02` | left / right |
| `03` | middle |
| `09` / `0a` | wheel up / down |
| `34` / `35` | wheel tilt left / right |
| `0b` / `0c` | DPI up / down |
| `0e` | profile button |
| `40`..`4b` | thumb grid 1..12 (`40 + n - 1`) |

## Action encoding

Every action is `[type, length of the data, data...]`, the same layout [razerqdhid](https://github.com/geezmolycos/razerqdhid/blob/main/docs/cmd_button-en.md) documents for the Basilisk V3.

| Type | Bytes | Meaning | Status |
|---|---|---|---|
| `00` off | `00 00` | button does nothing | inferred |
| `01` mouse | `01 01 <btn>` | `01` left, `02` right, `03` middle, `04` back, `05` forward, `09`/`0a` wheel, `68`/`69` wheel left/right | verified (left/right/wheel) |
| `02` keyboard | `02 02 <mod> <hid usage>` | `mod` is the HID modifier bitmask | verified |
| `06` DPI | `06 01 <op>` | `01` up, `02` down, `06`/`07` up/down with wrap | verified (read back as defaults) |
| `06` DPI stage | `06 02 03 <n>` | jump to stage n | razerqdhid, untested |
| `07` profile | `07 01 <op>` | `01` next, `02` previous, `04`/`05` with wrap, and the bottom button's default is `04` | read back, not pressed |
| `07` profile | `07 02 03 <n>` | jump to profile n | razerqdhid, untested |
| `0a` media | `0a 02 <usage u16 BE>` | HID consumer usage, `00 cd` play/pause, `00 b0` play, `00 b1` pause, `00 b5`/`00 b6` next/previous, `00 b7` stop | verified (pressed) |
| `0c` hypershift | `0c 01 01` | hold for the Hypershift layer | verified (pressed) |
| `0d` key turbo | `0d 04 <mod> <usage> <ms u16 BE>` | repeat a key every ms | verified (pressed) |
| `0e` mouse turbo | `0e 03 <btn> <ms u16 BE>` | repeat a button every ms | the tilt defaults read `0e 01 ...`, which [Unknowns](#unknowns) covers, and writing `03` reads back fine |

razerqdhid also documents macros (types `03`, `04`, `05`, `0f`, stored in class `0x06`), double-click (`0b`) and power control (`09`), but none of those are tested on the Naga, so for now they're only what razerqdhid found on the Basilisk V3.

## Lighting (class `0x0f`)

opensynapse writes lighting through OpenRazer, but the mouse also answers the extended matrix reads, so the GUI can show you what the LEDs are really doing.

| Command | Args | Reply | Status |
|---|---|---|---|
| `0x82` get effect | `[01, led]` | `[store, led, effect, direction/type, speed, colour count, r, g, b]`, the layout OpenRazer writes in `razerchromacommon.c` | verified, static green reads `00 <led> 01 00 00 01 00 ff 00` |
| `0x84` get brightness | `[01, led]` | `[01, led, brightness]` | verified (`ff`) |

LED ids are OpenRazer's, so `01` is the scroll wheel, `04` the logo and `10` the right side, and only effect `01` static is verified, while `00` none, `02` breathing, `03` spectrum, `04` wave and `05` reactive come from OpenRazer's write layout and haven't been read back from the mouse.

## Profiles (class `0x05`)

The mouse stores up to 5 profiles, but only profile `1` exists out of the box, so you have to create the others before the mouse uses them.

| Command | Args | Reply | Status |
|---|---|---|---|
| `0x80` get profile count | none, size `01` | number of created profiles | verified |
| `0x81` get profile list | none, size `0x50` | `[max, id, id, ...]`, the size counts the max too | verified, for example `05 01 02` with profiles 1 and 2 |
| `0x84` get active profile | none, size `01` | profile id | verified |
| `0x8a` get max profiles | none, size `01` | `05` | verified |
| `0x02` create profile | `[n]`, size `01` | status only | verified |
| `0x04` set active profile | `[n]`, size `01` | status only | verified, `0x84` reads `n` afterwards |
| `0x03` delete profile | `[n]`, size `01` | status only | verified, `0x81` drops `n` but its keymap stays on the mouse |

You can write keymaps to profiles `2`..`5` before they even exist, and creating a profile later keeps whatever you wrote there, but the firmware only switches between profiles you created. So if only profile `1` exists, the profile button (`07 01 04`) and a button set to `07 02 03 02` just do nothing, and as soon as you create profile 2 with `0x02`, pressing that same button switches to it, `0x84` starts reading `02` and profile `0` gets profile 2's buttons.

Profile `0` is the live copy of the active profile, so when you switch profiles the mouse copies the stored one into it and `0x84` reports the new id, anything you write to it applies right away and is gone when you unplug, and on a replug the mouse copies the active stored profile into it again.

## Unknowns

The mouse doesn't check anything you write to it, so you can send any type byte from `0x01` to `0x1f` and it stores it exactly like you sent it, the only ones it rewrites are `0x00` and `0x14` with a 6-byte payload. So if you set a button to a type the firmware doesn't know, say `0x1e`, the write goes through and reads back as `0x1e`, but the button might do nothing at all, which means a write that reads back fine tells you nothing about what the button does, and the only way to know is to press it and watch the input reports.

- Some defaults come back with a length that doesn't match the data, like the keys on the Hypershift layer and thumb9, which read `02 01 00 <usage>`, and the tilt defaults, which read `0e 01 <btn> 00 14`, but those buttons still work, so the firmware seems to ignore the length there, and if you write the documented length instead, that works too.
- If you set a button to `07 02 03 <n>` it switches to profile `n` once that profile exists, but nobody has tested the relative forms `07 01 0x` with more than one profile yet, so you can't count on them cycling the same way.
