# Razer Naga Left-Handed Edition 2020 keymap protocol

Device: USB `1532:008d`, firmware v1.0. Synapse 4 calls it device 141, edition 0 (`Nagalhe_UI`). Device file: [`razer-naga-left-handed-2020.toml`](../razer-naga-left-handed-2020.toml).

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

Layer `0` is normal and `1` is Hypershift. Profiles `0`..`2` answer. Writing profile `0` changes the live behaviour and leaves profile `1` untouched.

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

| Type | Bytes | Meaning |
|---|---|---|
| `01` mouse | `01 01 <btn>` | `01` left, `02` right, `03` middle, `09`/`0a` wheel |
| `02` keyboard | `02 <?> <mod> <hid usage>` | `mod` is the HID modifier bitmask |
| `06` DPI | `06 01 <n>` | `01` up, `02` down |
| `07` profile | `07 01 <n>` | `04` on the profile button (cycle?) |
| `0e` tilt | `0e 01 <n> 00 <rate>` | `0a`/`09` with `0x14` |

## Unknowns

The mouse doesn't validate actions on write: on thumb1 it stored every type byte from `0x01` to `0x1f` exactly as sent (only type `0x00`, and `0x14` with a 6-byte payload, came back rewritten). A write that's accepted and reads back fine says nothing about what the button will do, so new action types have to be checked by pressing the button and watching the input reports.

- The second keyboard byte. It's `02` on most profile 0/1 layer-0 keys and `01` elsewhere. Writing `02` worked.
- Whether profile 0 lives in RAM, and whether it survives a replug.
- Which command switches or persists the active profile.
- Macro storage (class `0x06` answers many ids with empty data).

## Tools

`tools/probe.py /dev/hidrawN [class ...] [--args hex]` sends GET commands only.
