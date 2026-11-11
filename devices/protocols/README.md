# Protocol files

Each file here is what we found out about one mouse by talking to it. It lists the transport, the commands, the button ids and how a button's action is encoded in bytes. The app doesn't read these files, they're for you, so you can check what a mouse does, try things by hand, or build something of your own on top of it.

| Mouse | Protocol | Device file |
|---|---|---|
| Razer Naga Left-Handed Edition 2020 | [razer-naga-left-handed-2020.md](razer-naga-left-handed-2020.md) | [../razer-naga-left-handed-2020.jsonc](../razer-naga-left-handed-2020.jsonc) |
| Razer Naga X | [razer-naga-x.md](razer-naga-x.md) | [../razer-naga-x.jsonc](../razer-naga-x.jsonc) |

The part that's the same on every Razer mouse, the 90-byte report and its checksum, is in [PROTOCOL.md](../../PROTOCOL.md#whats-the-same-on-every-razer-mouse). How to write a new file is in [PROTOCOL.md](../../PROTOCOL.md#finding-things-out) too.

## Contents

- [Manual reading](#manual-reading)
- [Explanation on the bytes](#explanation-on-the-bytes)
- [Changing a button](#changing-a-button)
- [Using it from Rust](#using-it-from-rust)
- [Creating a report](#creating-a-report)

## Manual reading

Every read is safe, it can't change anything on the mouse. The Naga file says the keymap is class `0x02` and that `0x8c` reads one button with the args `[profile, button, layer]`. So to read thumb1 (`0x40`) on the live profile `0` and the normal layer `0`:

```sh
opensynapse probe 0x02 --args 004000
```

With one Razer mouse plugged in, `probe` finds the right hidraw node by itself. On a Naga Left-Handed this prints, among the other reads of the class:

```text
using /dev/hidraw10 (Razer Naga Left Handed Edition, 1532:008d, interface 0, tid 0x1f)
firmware: status=ok v1.0
class 0x02 id 0x8c size= 10 data=0040000202001e
```

The reply starts by echoing the args, `00 40 00`, and the rest is the action, `02 02 00 1e`. The action encoding table in the Naga file says `02` is a keyboard key, `02` more bytes follow, `00` means no modifier and `1e` is the HID usage for `1`. So thumb1 types `1`, which is what `opensynapse get thumb1` shows:

```sh
opensynapse get thumb1
# key:1
```

## Explanation on the bytes

Every action in a protocol file has a matching action you can pass to `opensynapse set`:

| Bytes | Action |
|---|---|
| `02 02 00 1e` | `key:1` |
| `02 02 01 06` | `key:ctrl+c`, `01` is the ctrl bit and `06` is `c` |
| `01 01 04` | `mouse:back` |
| `0a 02 00 cd` | `media:playpause` |
| `06 01 01` | `dpi:up` |
| `07 01 04` | `profile:next-wrap` |
| `0c 01 01` | `hypershift` |
| `0e 03 01 00 32` | `turbo:left:50`, left click every 50 ms |
| `00 00` | `disabled` |

For bytes that don't have a name yet, like a type the protocol file lists as untested, use `raw:` with the hex:

```sh
opensynapse set thumb1 raw:0202001e   # the same as key:1
```

## Changing a button

A write changes the mouse, so do it on one button and keep a way back:

```sh
opensynapse export before.json         # everything on the live profile, all layers
opensynapse set thumb1 key:f13         # set reads it back and fails if it didn't stick
opensynapse get thumb1                 # key:f13
timeout 20 opensynapse watch           # press thumb1, check it really sends F13
opensynapse import before.json         # put everything back
```

Profile `0` is the live one and is lost when you unplug the mouse, so it's the safe one to try things on. Pass `--profile 1` to write the profile the mouse keeps.

> [!WARNING]
> A write that reads back fine doesn't prove anything. The Naga stores almost any byte you send without checking it, so only pressing the button tells you the encoding really works.

## Using it from Rust

The CLI and the GUI are built on a library you can use too. Add it to your `Cargo.toml`:

```toml
[dependencies]
opensynapse = { git = "https://github.com/khaosdoctor/opensynapse" }
```

Then send the same read as above. `command` takes the class, the command id, the data size from the protocol file and the args, and returns the whole 90-byte reply:

```rust
use opensynapse::action::Action;
use opensynapse::hid::{self, Hid};

fn main() -> std::io::Result<()> {
    // interface 0 of a Naga Left-Handed, from the device file's `interface`
    let path = hid::find(0x1532, 0x008d, 0).expect("mouse not plugged in");
    let mouse = Hid::open(&path, 0x1f)?;

    // class 0x02, read button 0x8c, size 0x0a, args [profile, button, layer]
    let reply = mouse.command(0x02, 0x8c, 0x0a, &[0x00, 0x40, 0x00])?;

    // args start at byte 8, the action comes after the 3 echoed ones
    let action = Action::decode(&reply[11..]);
    println!("thumb1 is {action}");
    Ok(())
}
```

`Action` also goes the other way, `"key:ctrl+c".parse::<Action>()?.encode()` gives you the bytes for a write.

## Creating a report

If you're not using Rust, here's the read above as the 90 bytes you send with the `HIDIOCSFEATURE` ioctl on the hidraw node:

| Byte | Value | Field |
|---|---|---|
| 0 | `00` | status, `00` when sending |
| 1 | `1f` | transaction id, from the device file |
| 2..4 | `00 00 00` | unused |
| 5 | `0a` | data size |
| 6 | `02` | command class |
| 7 | `8c` | command id, `0x80` set means it's a read |
| 8..10 | `00 40 00` | args, profile, button and layer |
| 11..87 | `00` | rest of the args |
| 88 | `c4` | XOR of bytes 2 to 87, here `0a ^ 02 ^ 8c ^ 40` |
| 89 | `00` | unused |

Then read the reply back with `HIDIOCGFEATURE` until byte 0, the status, stops being `01` busy. `02` means it worked, and the answer starts at byte 8. On Linux, hidraw wants one extra report id byte of `00` in front of both buffers, so they're 91 bytes long.
