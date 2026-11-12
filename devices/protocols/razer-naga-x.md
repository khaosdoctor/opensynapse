# Razer Naga X keymap protocol

This is the USB device `1532:0096` on firmware v1.0, and its device file is [`razer-naga-x.jsonc`](../razer-naga-x.jsonc).

> `opensynapse protocol` generated this from the device file and read-only queries to the mouse, so anything still marked TODO needs someone who has tested the mouse, as [PROTOCOL.md](../../PROTOCOL.md) explains.

If you run `opensynapse init` on this mouse it can't finish, because probing each interface makes it disconnect and reconnect, and that moves interface 3, the one that answers commands, to a new hidraw path before init is done, so the information here comes from `probe.py` instead.

## Contents

- [Transport](#transport)
- [Button ids](#button-ids)
- [DPI](#dpi)
- [Action encoding](#action-encoding)
- [Unknowns](#unknowns)

## Transport

It uses interface 3 with transaction id `0x1f`.

## Button ids

This is what each button does right now, read from profile 0, so these are only the defaults if nobody remapped them.

| Id | Button | normal | hypershift |
|---|---|---|---|
| `0x01` | left | `mouse:left` | `mouse:left` |
| `0x02` | right | `mouse:right` | `mouse:right` |
| `0x03` | scroll-click | `mouse:middle` | `mouse:middle` |
| `0x09` | wheel-up | `mouse:wheelup` | `mouse:wheelup` |
| `0x0a` | wheel-down | `mouse:wheeldown` | `mouse:wheeldown` |
| `0x60` | dpi-cycle | `mouse:middle` | `dpi:up-wrap` |
| `0x40` | thumb1 | `key:num1` | `key:1` |
| `0x41` | thumb2 | `key:num2` | `key:2` |
| `0x42` | thumb3 | `key:num3` | `key:3` |
| `0x43` | thumb4 | `key:num4` | `key:4` |
| `0x44` | thumb5 | `key:num5` | `key:5` |
| `0x45` | thumb6 | `key:num6` | `key:6` |
| `0x46` | thumb7 | `key:num7` | `key:7` |
| `0x47` | thumb8 | `key:num8` | `key:8` |
| `0x48` | thumb9 | `key:num9` | `key:9` |
| `0x49` | thumb10 | `key:num0` | `key:0` |
| `0x4a` | thumb11 | `key:numplus` | `key:minus` |
| `0x4b` | thumb12 | `key:numminus` | `key:equal` |

## DPI

The DPI reads 1800 x 1800 with a single stage at 1800, which is stage 1 and the active one, and the device file puts the max at 18000.

## Action encoding

The thumb buttons, the mouse clicks, the scroll wheel and the DPI switch are verified working, but the wheel click isn't, since it's physically broken on the mouse this file comes from, which is also why the DPI switch is set to middle click here.

## Unknowns

```text
❯ opensynapse watch
press buttons, ctrl+c to stop
-
scroll-click dpi-cycle
```

If you press dpi-cycle while it's set to middle click, `watch` shows the physical wheel click with it, which is the `scroll-click dpi-cycle` line in that output, and whether that's intended isn't known yet.
