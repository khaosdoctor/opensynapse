# AGENTS.md

opensynapse remaps buttons, sets DPI and drives lighting on Razer mice from Linux, without Synapse. It's one Rust crate with a library, a CLI (`opensynapse`) and a GUI (`opensynapse-gui`, behind the `gui` feature). Read this before you change anything, then [README.md](README.md) for what users see, [PROTOCOL.md](PROTOCOL.md) for the wire format and [CONTRIBUTING.md](CONTRIBUTING.md) if you're adding a mouse.

## Build and check

```sh
cargo build --features gui
cargo test --features gui                   # also runs the device-from-issue example's test
cargo clippy --features gui --all-targets   # keep it at zero warnings
cargo run --features gui --bin opensynapse-gui
```

The repo isn't formatted with rustfmt, so don't run `cargo fmt`, it rewrites every file. Only one GUI can run at a time, a second one shows an "already running" dialog.

## Talking to a real mouse

Everything here ends up as bytes on a real device, and a wrong write can leave a button doing something odd until someone fixes it by hand. So:

- Reads are always fine. `list`, `dump`, `get`, `export`, `watch`, `init`, `probe`, `protocol`, `battery`, `preset list|save|delete`, `profile` with no subcommand, `profile rename` and `poll` or `idle` with no value only read the mouse. `probe` refuses any command without the read bit.
- Before any write (`set`, `dpi`, `stages`, `rgb`, `poll <rate>`, `idle <seconds>`, `preset load`, `import`, `profile create|delete|switch`, or the GUI's WRITE TO DEVICE), say exactly what you're about to send and wait for a human to say yes. Change one thing, read it back, then put it back.
- Never run `opensynapse protocol send`, it posts a public issue.
- The exploration rules for new mice are in the agent prompt at the end of [CONTRIBUTING.md](CONTRIBUTING.md#using-an-agent).

## Testing without a mouse

The lighting, polling rate, battery, sleep timeout and presets all go through OpenRazer, and OpenRazer has fake devices for its own tests. `scripts/fake-devices.sh` creates them and runs a second daemon for them on a private session bus, so your real daemon and mouse never see any of it. Writes are fine here, nothing reaches real hardware.

```sh
scripts/fake-devices.sh                    # a Naga Left-Handed 2020 and a wireless Viper Ultimate, in a new shell
scripts/fake-devices.sh --list             # every model OpenRazer can fake
scripts/fake-devices.sh razerbasiliskv3 -- cargo run --bin opensynapse -- rgb
```

Inside that shell, `cargo run --bin opensynapse -- rgb`, `poll`, `battery`, `idle` and `preset ...` work on the fake devices, `--device 007b` picks one, and `cargo run --features gui --bin opensynapse-gui` shows them in the lighting tab. Exit the shell and the daemon and the fake files are gone.

The first run downloads the OpenRazer release that matches your `openrazer-daemon` to `~/.cache/opensynapse`, since the fake device generator only ships in its source. It needs `python3` and `dbus-run-session`.

The script sets `OPENSYNAPSE_OPENRAZER_TEST` to the fake devices' folder. Debug builds then list only those devices, read lighting from that folder instead of `/sys/bus/hid/drivers`, keep presets and other config inside it, and send nothing over hidraw. Release builds don't have any of this. Buttons and DPI need hidraw, so on a fake device they fail with "fake OpenRazer devices have no hidraw node". Test those on a real mouse.

Debug builds also read `OPENSYNAPSE_NO_PROFILES`. When it is set, the GUI treats the mouse as if it didn't answer the profile commands, so you can try the presets on a mouse that has onboard profiles.

## Where things are

| Path | What's in it |
|---|---|
| `src/lib.rs` | the library the CLI and GUI share |
| `src/hid.rs` | the 90-byte Razer feature report over hidraw, finding the right interface, and the trace hook (`hid::trace`) that reports every command and reply, with a read only reported when its answer changes |
| `src/trace.rs` | turns a traced command into words using the device file's button and layer names, and returns `None` for commands nobody has decoded so they show as raw bytes |
| `src/action.rs` | what a button does and how it's encoded on the wire |
| `src/device.rs` | device files, the config folder, the JSON helpers everything else uses, and the commands for buttons, DPI and stored profiles |
| `src/profile.rs` | profile export/import files, profile names, which only exist on this computer, and the paths of the presets and the active preset |
| `src/input.rs` | which buttons are held right now, for `watch` and the GUI highlights |
| `src/rgb.rs` | lighting through the OpenRazer driver and daemon, we don't reverse engineer lighting |
| `src/power.rs` | polling rate, battery and sleep timeout through the OpenRazer daemon |
| `src/preset.rs` | named lighting presets in `~/.config/opensynapse/presets` |
| `src/fake.rs` | OpenRazer's fake devices for debug builds, see [Testing without a mouse](#testing-without-a-mouse) |
| `src/scaffold.rs` | `init`, which starts a device file for an unknown mouse using reads only |
| `src/generators/protocol.rs` | writes `devices/protocols/<slug>.md` for `opensynapse protocol` |
| `src/main.rs` | the CLI, including `probe` for raw read-only probing of a hidraw node |
| `src/bin/opensynapse-gui/` | the GUI, see below |
| `examples/device-from-issue.rs` | turns an issue from `protocol send` into `devices/<slug>.jsonc` and its protocol file for `add-device.yml`. It's an example so `cargo install` and the packages never ship it |
| `devices/*.jsonc` | one file per supported mouse, `devices/protocols/*.md` documents what was found, and `devices/protocols/README.md` shows how to use those files by hand and from Rust |
| `themes/*.css` | built-in GUI themes, `opensynapse.css` lists every property |
| `udev/` | the rule that lets your user open the mouse without root |
| `contrib/` | AUR PKGBUILDs, the Homebrew formula and shell completions |

The GUI is split by job, and every file stays under 500 lines:

| Path | What's in it |
|---|---|
| `main.rs` | module list, the single-instance check and `main()` |
| `state.rs` | the `App` struct, the pending queue (`Op`) and what each button will do |
| `lib/protocol.rs` | everything the GUI does with the mouse or the OpenRazer daemon |
| `lib/remapper.rs` | the REMAP editor, action types and key lists |
| `lib/lighting.rs` | the lighting tab, with the polling rate, battery and sleep timeout |
| `lib/presets.rs` | the preset list, SAVE AS and DELETE on the lighting tab |
| `lib/profiles.rs` | the profile list, names and active profile with the queued changes applied |
| `lib/profile_presets.rs` | profiles as presets saved on this computer, for mice that don't answer the profile commands |
| `lib/debug.rs` | the session debug log, written to the journal, to `$XDG_STATE_HOME/opensynapse/debug.log` and to the LOG panel in the sidebar |
| `lib/instance_lock.rs` | the lock file and the "already running" dialog |
| `gui/` | layout, the mouse drawing, the sidebar and the dialogs |
| `shared/widgets.rs` | every styled widget. Views ask it for a button or a panel and never pick colors themselves |
| `theme/` | palette, the CSS parser, settings.json and the theme list, light/dark detection through xdg-desktop-portal |

If something is used by one module it stays in that module, once two modules use it, it moves to `shared/`.

## How devices and themes load

Device files are read when the app starts, nothing about a mouse is compiled in. `device::definitions()` looks in this order, and the first file for a vid:pid wins:

1. `~/.config/opensynapse/devices/<vid-pid>/buttons.jsonc`, so a user's file wins over the shipped one. This is how people try a new mouse before a release.
2. `opensynapse/devices/*.jsonc` under every `$XDG_DATA_DIRS` entry (`/usr/local/share:/usr/share` when it's unset), where the `.deb` and the AUR packages put them.
3. `../share/opensynapse/devices/*.jsonc` and `devices/*.jsonc` next to the binary, for the AppImage, Homebrew and the tar.gz.
4. `devices/` in the checkout the binary was built from (`CARGO_MANIFEST_DIR`), so `cargo run`, the tests and `cargo install --path .` work without installing anything.

A file's slug, the name its protocol file uses, is its file name without `.jsonc`. User files are all called `buttons.jsonc`, so theirs comes from the `name` field. Adding a mouse means adding `devices/<slug>.jsonc` and nothing else.

Themes are different. `themes/*.css` are compiled in, and every `~/.config/opensynapse/themes/<name>.css` becomes a button on the settings page, replacing a built-in theme with the same name. Only CSS custom properties are read, see the header of `themes/opensynapse.css`. `~/.config/opensynapse/theme.css` overrides single properties on top of any theme, and `settings.json` keeps the mode, theme, font and whether the log shows raw bytes (`raw_bytes`).

The GUI's debug log is the place to look when something goes wrong. It goes to `$XDG_STATE_HOME/opensynapse/debug.log` (emptied on every start, deleted on a clean exit) and to the journal, `journalctl -t opensynapse-gui -f`. Anything new that talks to the mouse already shows up through `hid::trace`, and writes that skip hidraw call `hid::note`. To translate a new command, add it to `src/trace.rs` with a test.

## Releases and CI

- `release.yml` runs on a `v*` tag. It checks the tag matches `Cargo.toml`, builds the tar.gz, `.deb` and AppImage for x86_64 and aarch64, writes the release notes from the commits with git-cliff, then publishes to the AUR, Homebrew and the apt repo on the `apt-repo` branch.
- `add-device.yml` runs when an issue titled `New Device:` is opened or edited, which is what `opensynapse protocol send` creates, or when someone adds the `add-device` label by hand. It labels the issue `add-device`, runs `device-from-issue`, which checks the device file with the app's parser and writes it, then opens a PR labeled `new-device`. Anyone can open such an issue, so the body is only ever parsed as data, and the PR still needs a maintainer's review to merge.
- `label-device.yml` adds the `new-device` label to any PR whose title starts with `New Device:`, since contributors can't label their own PRs. `protocol send` issues use the same prefix.
- `device-release.yml` ships a patch release when a PR labeled `new-device` is merged.
- `pages.yml` publishes `docs/` to the website.

Every package ships `devices/*.jsonc`. The `.deb`, the AUR packages, the AppImage and Homebrew install them to `share/opensynapse/devices`, and the tar.gz keeps them in `devices/` next to the binaries. Without them an installed app finds no mouse, so a new package has to install them too.

To cut a release, run `scripts/release.sh patch` (or `minor`, or an exact `X.Y.Z`) on a clean `main`. It bumps `Cargo.toml`, `Cargo.lock` and the three PKGBUILDs, runs the tests, commits `chore: release vX.Y.Z`, tags it and pushes, and the tag starts `release.yml`.

## Conventions

- Commits are [conventional commits](https://www.conventionalcommits.org) with a subject line only, like `fix(gui): keep the editor tab when picking another button`. Don't add `Co-Authored-By` or any other AI attribution line, and keep the human as author.
- Prefer early returns over `else`, avoid `let mut` when you can, use plain `for` loops over long iterator chains, and don't add a trait or abstraction for something that has one implementation.
- Comments say why, or something the code doesn't show. Don't restate the function name.
- Docs describe how things work now, not what changed. Keep README, CONTRIBUTING and this file in sync when you change behavior they describe.
- Nothing about a specific mouse goes in the code. If one model needs something, it goes in its device file.
