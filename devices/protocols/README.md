# Protocol files

Each file here is what people found out about one mouse by sending it commands. It lists the transport, the commands, the button ids and how a button's action is encoded in bytes. The app doesn't read these files, it's mainly for other people like me who want to add support for a new mouse without having to reverse engineer the protocol themselves. The protocol file is also useful for debugging and understanding how the device works.

> Expand the list below if you add yours

| Mouse | Protocol | Device file |
|---|---|---|
| Razer Naga Left-Handed Edition 2020 | [razer-naga-left-handed-2020.md](razer-naga-left-handed-2020.md) | [../razer-naga-left-handed-2020.jsonc](../razer-naga-left-handed-2020.jsonc) |
| Razer Naga X | [razer-naga-x.md](razer-naga-x.md) | [../razer-naga-x.jsonc](../razer-naga-x.jsonc) |

The part that's the same on every Razer mouse is in [PROTOCOL.md](../../PROTOCOL.md#whats-the-same-on-every-razer-mouse). How to write a new file is in [PROTOCOL.md](../../PROTOCOL.md#finding-things-out) too.

## Contents

- [Manual reading](#manual-reading)
- [Explanation on the bytes](#explanation-on-the-bytes)
- [Changing a button](#changing-a-button)
- [Using it from Rust](#using-it-from-rust)
- [Creating a report](#creating-a-report)

## Manual reading

To start it out we read stuff, it's safe, it can't change anything on the mouse. The Naga file, for example, has the keymap which says the class is `0x02` and that `0x8c` reads one button with the args `[profile, button, layer]`. So, to read thumb1 (`0x40`) on the live profile `0` and the normal layer `0` we would run:

```sh
opensynapse probe 0x02 --args 004000
```

With one Razer mouse plugged in, `probe` finds the right hidraw node by itself. In the Naga LE, this prints, among the other reads of the class:

```text
using /dev/hidraw10 (Razer Naga Left Handed Edition, 1532:008d, interface 0, tid 0x1f)
firmware: status=ok v1.0
class 0x02 id 0x8c size= 10 data=0040000202001e
```

The reply starts by echoing the args, `00 40 00`, and the rest is the action, `02 02 00 1e`. To decode this we need the encoding table.

The action encoding table in the Naga file says `02` is a keyboard key. So this is a keyboard key action (if we press it, it will press a key on the keyboard), then `02` means there are 2 more bytes to be read (`00` and `1e`). `00` means no modifier (so, a simple key press, no `ctrl` or similar) and `1e` is the HID usage for `1`. So, pressing `thumb1` types `1`, which is what `opensynapse get thumb1` shows:

```sh
opensynapse get thumb1
# key:1
```

So, the bytes are defined:

```text
02 02 00 1e
^  ^  ^  ^
|  |  |  +-- the key, 1 in this case
|  |  +----- the modifier, 00 for none
|  +-------- number of bytes after this
+----------- the type, 02 for keyboard key
```

> You can figure out these actions by reading the [PROTOCOL.md](../../PROTOCOL.md#finding-things-out) section, or by reading the [Naga Left-Handed 2020 file](razer-naga-left-handed-2020.md) which has the full table. But the actions are not 100% certain to be the same on every mouse, so if you want to be sure, read the button and see what it does. It's really a trial and error process, this is why it's called **reverse** engineering

## Explanation on the bytes

Every action in a protocol file has a matching action you can pass to `opensynapse set`, up to now, these are some examples that we have discovered:

| Bytes | Action |
|---|---|
| `02 02 00 1e` | `key:1` |
| `02 02 01 06` | `key:ctrl+c`, `01` is the ctrl bit and `06` is `c` |
| `01 01 04` | `mouse:back`, the second `01` says there's only one more byte after it, the `04` |
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
opensynapse set thumb1 key:f13         # setting something also reads it back and errors out if it wasn't persisted
opensynapse get thumb1                 # key:f13
timeout 20 opensynapse watch           # press thumb1, check it really sends F13
opensynapse import before.json         # writes the backup back
```

Profile `0` is the live one and is lost when you unplug the mouse (verified on the Naga), so it's the safe one to try things on. In any case, back things up before trying them out. Pass `--profile 1` to write the profile the mouse keeps.

> [!WARNING]
> Writing and reading it back without errors doesn't prove anything. The Naga stores almost any byte you send without checking it, which means it stores invalid ones too... So the only real test is to actually press the button.

## Using it from Rust

The CLI and the GUI are built on a library you can use too. Add it to your `Cargo.toml`:

```toml
[dependencies]
opensynapse = { git = "https://github.com/khaosdoctor/opensynapse" }
```

Then send the same read as above. `command` takes the class, the command id, the data size from the protocol file and the args. It returns the whole 90-byte response:

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

If you're not using Rust, there's this table which is the read above as the 90 bytes you send with the `HIDIOCSFEATURE` ioctl on the hidraw node:

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
