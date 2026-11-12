# Razer Naga Left-Handed Edition 2020 keymap protocol

Device: USB `1532:008d`, firmware v1.0. Synapse 4 calls it device 141, edition 0 (`Nagalhe_UI`). Device file: [`razer-naga-left-handed-2020.jsonc`](../razer-naga-left-handed-2020.jsonc).

All findings come from talking to the real device on Linux through `/dev/hidraw` on interface 0. Synapse was never run, and its shipped UI, installer and driver package don't contain the packet encoder.

## Contents

- [Transport](#transport)
- [Keymap commands (class `0x02`)](#keymap-commands-class-0x02)
- [Button ids](#button-ids)
- [Action encoding](#action-encoding)
- [Lighting (class `0x0f`)](#lighting-class-0x0f)
- [Profiles (class `0x05`)](#profiles-class-0x05)
- [Unknowns](#unknowns)

## Transport

The common frame from [PROTOCOL.md](../../PROTOCOL.md#whats-the-same-on-every-razer-mouse), on interface 0 with transaction id `0x1f`.

## Keymap commands (class `0x02`)

| Command | Args | Status |
|---|---|---|
| `0x8c` get button | `[profile, button, layer]` | reply echoes args, then the action. Verified. |
| `0x0c` set button | `[profile, button, layer, action...]`, size `0x0a` | Verified: status ok, read-back matches, keypress observed. |
| `0x84` list buttons | none | `count` + button ids |
| `0x8e` bulk dump | `[profile, button, layer]` | 8-byte entries, layout not decoded |
| `0x82` | none | `08 1c 08 1c`, meaning unknown |

Layer `0` is normal and `1` is Hypershift. Profiles `0`..`5` answer reads. Profile `0` is the live profile: writing it changes the behaviour right away, leaves profile `1` untouched, and is lost on a replug (verified). Writing profile `1` while it's the active profile also changes the behaviour right away and survives a replug (verified). Profiles `2`..`5` can be written before they exist, see [Profiles](#profiles-class-0x05).

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
| `07` profile | `07 01 <op>` | `01` next, `02` previous, `04`/`05` with wrap. The bottom button's default is `04` | read back, not pressed |
| `07` profile | `07 02 03 <n>` | jump to profile n | razerqdhid, untested |
| `0a` media | `0a 02 <usage u16 BE>` | HID consumer usage, `00 cd` play/pause, `00 b0` play, `00 b1` pause, `00 b5`/`00 b6` next/previous, `00 b7` stop | verified (pressed) |
| `0c` hypershift | `0c 01 01` | hold for the Hypershift layer | verified (pressed) |
| `0d` key turbo | `0d 04 <mod> <usage> <ms u16 BE>` | repeat a key every ms | verified (pressed) |
| `0e` mouse turbo | `0e 03 <btn> <ms u16 BE>` | repeat a button every ms | the tilt defaults read `0e 01 ...` (see below), writing `03` reads back fine |

razerqdhid also documents macros (types `03`, `04`, `05`, `0f`, stored in class `0x06`), double-click (`0b`) and power control (`09`), none of them tried on the Naga.

## Lighting (class `0x0f`)

Lighting is written through OpenRazer, but the mouse answers the extended matrix reads, so the GUI shows what the LEDs really do.

| Command | Args | Reply | Status |
|---|---|---|---|
| `0x82` get effect | `[01, led]` | `[store, led, effect, direction/type, speed, colour count, r, g, b]`, the layout OpenRazer writes in `razerchromacommon.c` | verified: static green reads `00 <led> 01 00 00 01 00 ff 00` |
| `0x84` get brightness | `[01, led]` | `[01, led, brightness]` | verified (`ff`) |

LED ids are OpenRazer's: `01` scroll wheel, `04` logo, `10` right side. Effect `01` static is verified; `00` none, `02` breathing, `03` spectrum, `04` wave and `05` reactive are decoded from OpenRazer's write layout and weren't read back.

## Profiles (class `0x05`)

The mouse stores up to 5 profiles. Only profile `1` exists out of the box, the others have to be created.

| Command | Args | Reply | Status |
|---|---|---|---|
| `0x80` get profile count | none, size `01` | number of created profiles | verified: `01`, then `02` after creating profile 2 |
| `0x81` get profile list | none, size `0x50` | `[max, id, id, ...]`, the size counts the max too | verified: `05 01` with only profile 1, `05 01 02` after creating profile 2 |
| `0x84` get active profile | none, size `01` | profile id | verified: `01`, then `02` after switching |
| `0x8a` get max profiles | none, size `01` | `05` | verified |
| `0x02` create profile | `[n]`, size `01` | status only | verified |
| `0x04` set active profile | `[n]`, size `01` | status only | verified: status ok, then `0x84` reads `n` |
| `0x03` delete profile | `[n]`, size `01` | status only | verified: `0x81` drops `n`, and its keymap still reads back unchanged |

Keymaps can be written to profiles `2`..`5` before they're created, and creating a profile leaves a keymap already written to it untouched. The firmware only switches between created profiles though. While only profile `1` existed, the profile button (`07 01 04`) and a button set to `07 02 03 02` did nothing. After `0x02` created profile 2, pressing the `07 02 03 02` button switched to it: `0x84` read `02` and profile `0` held profile 2's buttons.

Profile `0` is the live copy of the active profile. Switching copies the stored profile into profile `0`, writes to profile `0` apply right away and are lost on unplug, and on a replug the active stored profile is copied into it again.

After switching back to profile 1 with `0x04`, its thumb buttons read as `key:1`..`key:8`, which are profile 2's values, while left and right kept profile 1's swap. The cause isn't known.

## Unknowns

The mouse doesn't validate actions on write: on thumb1 it stored every type byte from `0x01` to `0x1f` exactly as sent (only type `0x00`, and `0x14` with a 6-byte payload, came back rewritten). A write that's accepted and reads back fine says nothing about what the button will do, so new action types have to be checked by pressing the button and watching the input reports.

- Some defaults read back with a length that doesn't match the data: keys on the Hypershift layer and thumb9 read `02 01 00 <usage>`, and the tilt defaults read `0e 01 <btn> 00 14`. They work, so the firmware seems to ignore the length on those. Writing the documented length works too.
- A button set to `07 02 03 <n>` switches to profile `n` once it exists (verified with `n` = 2). The relative forms `07 01 0x` were only seen doing nothing while one profile existed.
