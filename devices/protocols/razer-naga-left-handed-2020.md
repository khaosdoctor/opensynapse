# Razer Naga Left-Handed Edition 2020 keymap protocol

Device: USB `1532:008d`, firmware v1.0. Synapse 4 calls it device 141, edition 0 (`Nagalhe_UI`). Device file: [`razer-naga-left-handed-2020.json`](../razer-naga-left-handed-2020.json).

All findings come from talking to the real device on Linux through `/dev/hidraw` on interface 0. Synapse was never run, and its shipped UI, installer and driver package don't contain the packet encoder.

## Transport

90-byte HID feature report on interface 0, no report ID. It's the same frame openrazer uses:

| Offset | Field |
|---|---|
| 0 | status (`02` ok, `01` busy, `03` fail, `05` unsupported) |
| 1 | transaction id (`0x1f` works) |
| 5 | data size |
| 6 | command class |
| 7 | command id (bit `0x80` = get) |
| 8..87 | args |
| 88 | XOR of bytes 2..87 |

## Keymap commands (class `0x02`)

| Command | Args | Status |
|---|---|---|
| `0x8c` get button | `[profile, button, layer]` | reply echoes args, then the action. Verified. |
| `0x0c` set button | `[profile, button, layer, action...]`, size `0x0a` | Verified: status ok, read-back matches, keypress observed. |
| `0x84` list buttons | none | `count` + button ids |
| `0x8e` bulk dump | `[profile, button, layer]` | 8-byte entries, layout not decoded |
| `0x82` | none | `08 1c 08 1c`, meaning unknown |

Layer `0` is normal and `1` is Hypershift. Profiles `0`..`5` answer reads, writes were only tried on `0`. Writing profile `0` changes the live behaviour and leaves profile `1` untouched.

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
| `0a` media | `0a 02 <usage u16 BE>` | HID consumer usage, `00 cd` play/pause, `00 e9` volume up | razerqdhid, untested |
| `0c` hypershift | `0c 01 01` | hold for the Hypershift layer | razerqdhid, untested |
| `0d` key turbo | `0d 04 <mod> <usage> <ms u16 BE>` | repeat a key every ms | razerqdhid, untested |
| `0e` mouse turbo | `0e 03 <btn> <ms u16 BE>` | repeat a button every ms | the tilt defaults read `0e 01 ...` (see below), writing `03` reads back fine |

razerqdhid also documents macros (types `03`, `04`, `05`, `0f`, stored in class `0x06`), double-click (`0b`) and power control (`09`), none of them tried on the Naga.

## Unknowns

The mouse doesn't validate actions on write: on thumb1 it stored every type byte from `0x01` to `0x1f` exactly as sent (only type `0x00`, and `0x14` with a 6-byte payload, came back rewritten). A write that's accepted and reads back fine says nothing about what the button will do, so new action types have to be checked by pressing the button and watching the input reports.

- Some defaults read back with a length that doesn't match the data: keys on the Hypershift layer and thumb9 read `02 01 00 <usage>`, and the tilt defaults read `0e 01 <btn> 00 14`. They work, so the firmware seems to ignore the length on those. Writing the documented length works too.
- Profile 0 should be the "direct" profile, applied right away but lost on power off, with profiles 1 to 5 stored on the mouse (razerqdhid). On the Naga, writing profile 0 changed the live behaviour and left profile 1 alone, the replug part is untested.
- No command to set the active profile is known. A button set to `07 02 03 <n>` should switch profiles.
- `0x02/0x82` and `0x02/0x8e` aren't documented anywhere we looked.

## Tools

`tools/probe.py /dev/hidrawN [class ...] [--args hex]` sends GET commands only.
