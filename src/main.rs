use opensynapse::action::{self, Action};
use opensynapse::device;
use opensynapse::rgb::{self, Effect};
use opensynapse::generators::protocol;
use opensynapse::{hid, input, power, preset, profile, scaffold};
use std::error::Error;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::mpsc;
use std::time::Duration;

const USAGE: &str = "\
usage: opensynapse <command> [--profile N] [--layer NAME] [--device PID]

commands:
  init [pid] [file.jsonc]      start a device file for a mouse that doesn't have one (read-only)
  probe [/dev/hidrawN] [class ...] [--args HEX] [--tid HEX]
                               send every read command of each class (default 0x00-0x0f)
                               and print the ones that answer (read-only). Without a node it
                               finds the mouse and its tid, with one the tid defaults to 0x1f
  list                         connected mice and their buttons
  dump                         every button's action
  get <button>                 one button's action
  set <button> <action>        remap a button
  dpi [X [Y]]                  show or set DPI
  stages [DPI ...] [--active N]  show or set DPI stages (max 5, N counts from 1)
  watch                        print buttons as you press them
  rgb                          lighting zones (needs the OpenRazer driver)
  rgb <zone|all> <effect>      off | static RRGGBB | breath RRGGBB|random | spectrum
                               | wave [reverse] | reactive RRGGBB [speed 1-3]
  rgb <zone|all> brightness N  0-255
  preset list|save|load|delete [NAME]
                               lighting presets, load skips zones this mouse doesn't have
  poll [125|500|1000]          show or set the polling rate in Hz (needs the OpenRazer daemon)
  battery                      battery level, charging and the sleep timeout (wireless mice)
  idle [SECONDS]               show or set how long a wireless mouse waits before it sleeps (60-900)
  profile                      stored profiles and which one is active
  profile create|delete|switch N
                               create copies the active profile, the profile button only reaches created ones
  profile rename N NAME        a name for the GUI, kept on this computer (empty NAME removes it)
  export <file.json>           save keymap (all layers) + DPI of --profile
  import <file.json>           write a saved profile to --profile
  protocol [file.md]           generate a protocol file skeleton for this mouse (--force overwrites)
  protocol send [file.md]      open a GitHub issue with the protocol and device file
                               (--dry-run prints it instead, --yes skips the confirmation)

actions:
  key:[mods+]name   e.g. key:ctrl+1, key:f13, key:shift+a (mods: ctrl shift alt meta rctrl ...)
  mouse:name        left right middle back forward wheelup wheeldown wheelleft wheelright
  turbo:button:ms   repeat a mouse button every ms milliseconds
  keyturbo:[mods+]key:ms  repeat a key every ms milliseconds
  media:name        playpause play pause next prev stop mute volumeup volumedown
  hypershift        hold for the Hypershift layer
  dpi:up|down|up-wrap|down-wrap|stageN, profile:next|prev|next-wrap|prev-wrap|N,
  disabled, raw:hex

profile defaults to 0, the live copy of the active profile, lost on unplug. Layer to normal, device to the first mouse in `list`
(PID is the hex after the colon, like 008d).";

#[derive(Default)]
struct Opts {
    args: Vec<String>,
    profile: u8,
    layer: String,
    active: Option<u8>,
    device: Option<u16>,
    force: bool,
    dry_run: bool,
    yes: bool,
}

fn parse() -> Result<Opts, Box<dyn Error>> {
    let mut o = Opts { layer: "normal".into(), ..Default::default() };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut val = || it.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--profile" => o.profile = val()?.parse().map_err(|_| "bad --profile")?,
            "--layer" => o.layer = val()?,
            "--active" => o.active = Some(val()?.parse().map_err(|_| "bad --active")?),
            "--device" => o.device = Some(u16::from_str_radix(&val()?, 16).map_err(|_| "bad --device, use the pid like 008d")?),
            "--force" => o.force = true,
            "--dry-run" => o.dry_run = true,
            "--yes" | "-y" => o.yes = true,
            "-h" | "--help" => o.args = vec!["help".into()],
            _ => o.args.push(a),
        }
    }
    Ok(o)
}

fn run(o: Opts) -> Result<(), Box<dyn Error>> {
    let args: Vec<&str> = o.args.iter().map(String::as_str).collect();
    if matches!(args.first(), None | Some(&"help")) {
        println!("{USAGE}");
        return Ok(());
    }
    if args.first() == Some(&"init") {
        return init(&args[1..], o.force);
    }
    if args.first() == Some(&"probe") {
        return probe(&args[1..]);
    }
    // presets belong to the user, these two work without a mouse
    match args.as_slice() {
        ["preset", "list"] => {
            for name in preset::list() {
                println!("{name}");
            }
            return Ok(());
        }
        ["preset", "delete", name] => return Ok(preset::delete(name)?),
        _ => {}
    }
    let mice = device::detect()?;
    let mouse = match o.device {
        Some(pid) => mice.iter().find(|m| m.def.pid == pid).ok_or(format!("no supported mouse with pid {pid:04x}, see `opensynapse list`"))?,
        None => mice.first().ok_or("no supported Razer mouse found (if a device file was skipped above, fix it; otherwise run `opensynapse init`)")?,
    };
    let def = &mouse.def;
    if !def.profiles.contains(&o.profile) {
        return Err(format!("unknown profile {}, have {:?}", o.profile, def.profiles).into());
    }
    let layer = def.layer(&o.layer).ok_or(format!("unknown layer '{}', have {:?}", o.layer, def.layers))?;
    let button = |name: &str| def.button(name).map(|b| b.id).ok_or(format!("unknown button '{name}'"));
    let daemon = || power::find(&mouse.serial).ok_or("this needs the OpenRazer daemon running");
    let lighting = || rgb::find(def.vid, def.pid, &mouse.serial).ok_or("no lighting found (is the OpenRazer driver loaded?)");

    match args.as_slice() {
        ["list"] => {
            for m in &mice {
                let names: Vec<&str> = m.def.buttons.iter().map(|b| b.name.as_str()).collect();
                println!("{} ({:04x}:{:04x})\n  buttons: {}", m.def.name, m.def.vid, m.def.pid, names.join(" "));
            }
        }
        ["dump"] => {
            for b in &def.buttons {
                println!("{:<12} {}", b.name, mouse.get_button(o.profile, b.id, layer)?);
            }
        }
        ["get", name] => println!("{}", mouse.get_button(o.profile, button(name)?, layer)?),
        ["set", name, action] => {
            let action: Action = action.parse()?;
            mouse.set_button(o.profile, button(name)?, layer, &action)?;
            println!("{name} = {action}");
        }
        ["dpi"] => {
            let (x, y) = mouse.dpi()?;
            println!("{x} x {y}");
        }
        ["dpi", rest @ ..] if rest.len() <= 2 => {
            let x: u16 = rest[0].parse().map_err(|_| "bad DPI")?;
            let y: u16 = rest.get(1).map_or(Ok(x), |v| v.parse()).map_err(|_| "bad DPI")?;
            mouse.set_dpi(x, y)?;
            println!("{x} x {y}");
        }
        ["stages", rest @ ..] => {
            let mut s = mouse.dpi_stages()?;
            if !rest.is_empty() {
                s.stages = rest.iter().map(|v| v.parse()).collect::<Result<_, _>>().map_err(|_| "bad DPI")?;
            }
            s.active = o.active.unwrap_or(s.active);
            if !rest.is_empty() || o.active.is_some() {
                s.active = s.active.min(s.stages.len() as u8);
                mouse.set_dpi_stages(&s)?;
            }
            for (i, dpi) in s.stages.iter().enumerate() {
                let mark = if i as u8 + 1 == s.active { "*" } else { " " };
                println!("{mark}{} {dpi}", i + 1);
            }
        }
        ["watch"] => {
            let actions: Vec<Option<Action>> =
                def.buttons.iter().map(|b| mouse.get_button(o.profile, b.id, layer).ok()).collect();
            let (tx, rx) = mpsc::channel();
            let state = input::listen(def.vid, def.pid, move || {
                let _ = tx.send(());
            });
            println!("press buttons, ctrl+c to stop");
            let mut last = String::new();
            // the timeout lets a wheel notch expire without another event
            while !matches!(rx.recv_timeout(Duration::from_millis(200)), Err(mpsc::RecvTimeoutError::Disconnected)) {
                let Ok(s) = state.lock() else { break };
                let held: Vec<&str> = def
                    .buttons
                    .iter()
                    .zip(&actions)
                    .filter(|(_, a)| a.as_ref().is_some_and(|a| s.triggers(a)))
                    .map(|(b, _)| b.name.as_str())
                    .collect();
                let line = if held.is_empty() { "-".to_string() } else { held.join(" ") };
                if line != last {
                    println!("{line}");
                    last = line;
                }
            }
        }
        ["rgb", rest @ ..] => {
            let light = lighting()?;
            let Some((zone, rest)) = rest.split_first() else {
                println!("via {}", if light.uses_daemon() { "openrazer daemon" } else { "openrazer driver (sysfs)" });
                for z in &light.zones {
                    let effect = light.current_effect(z).map_or("?".into(), |e| format!("{e:?}"));
                    println!("{z:<10} brightness {:>3}  effect {effect}", light.brightness(z)?);
                }
                return Ok(());
            };
            let zones: Vec<&str> = match *zone {
                "all" => light.zones.iter().map(String::as_str).collect(),
                z if light.zones.iter().any(|x| x == z) => vec![z],
                z => return Err(format!("unknown zone '{z}', have {:?} or all", light.zones).into()),
            };
            match rest {
                ["brightness", v] => {
                    let v: u8 = v.parse().map_err(|_| "brightness is 0-255")?;
                    for z in &zones {
                        light.set_brightness(z, v)?;
                    }
                }
                [name, args @ ..] => {
                    let effect = parse_effect(name, args)?;
                    for z in &zones {
                        light.set_effect(z, &effect)?;
                    }
                }
                [] => return Err("rgb <zone|all> <effect|brightness> ...".into()),
            }
            println!("{} > {}", zones.join(","), rest.join(" "));
        }
        ["preset", "save", name] => println!("saved {}", preset::save(name, &lighting()?.state(mouse))?.display()),
        ["preset", "load", name] => {
            let light = lighting()?;
            let saved = preset::load(name)?;
            let total = saved.len();
            let zones = preset::for_zones(saved, &light.zones);
            for z in &zones {
                light.set_brightness(&z.zone, z.brightness)?;
                light.set_effect(&z.zone, &z.effect)?;
            }
            println!("loaded {} of {total} zones", zones.len());
        }
        ["poll", rest @ ..] if rest.len() <= 1 => {
            let power = daemon()?;
            if let Some(hz) = rest.first() {
                power.set_poll_rate(hz.parse().map_err(|_| "bad polling rate")?)?;
            }
            println!("{} Hz", power.poll_rate().ok_or("this mouse has no polling rate on the daemon")?);
        }
        ["battery"] => {
            let power = daemon()?;
            let b = power.battery().ok_or("this mouse has no battery")?;
            let idle = power.idle_time().map_or(String::new(), |s| format!("\nsleeps after {s} s idle"));
            println!("battery {:.0}%{}{idle}", b.percent, if b.charging { ", charging" } else { "" });
        }
        ["idle", rest @ ..] if rest.len() <= 1 => {
            let power = daemon()?;
            if let Some(s) = rest.first() {
                power.set_idle_time(s.parse().map_err(|_| "bad seconds")?)?;
            }
            println!("sleeps after {} s idle", power.idle_time().ok_or("this mouse has no sleep timeout")?);
        }
        ["protocol", "send", rest @ ..] if rest.len() <= 1 => {
            let path = rest.first().map_or_else(|| protocol_path(def), PathBuf::from);
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("{}: {e} > run `opensynapse protocol` first", path.display()))?;
            let (title, body) = protocol::issue(&text, mouse);
            if o.dry_run {
                println!("title: {title}\n\n{body}");
                return Ok(());
            }
            send_issue(&path, &title, &body, o.yes)?;
        }
        ["protocol", rest @ ..] if rest.len() <= 1 => {
            let path = rest.first().map_or_else(|| protocol_path(def), PathBuf::from);
            write_new(&path, &protocol::generate(mouse), o.force)?;
            println!("wrote {}\nfill in the TODOs, then `opensynapse protocol send` opens an issue with it", path.display());
        }
        ["profile", rest @ ..] => {
            let n = |s: &str| s.parse::<u8>().map_err(|_| format!("bad profile '{s}'"));
            match rest {
                [] => {}
                ["create", p] => {
                    let active = mouse.profiles()?.active;
                    mouse.create_profile(n(p)?)?;
                    profile::copy(mouse, active, n(p)?)?;
                }
                ["delete", p] => {
                    mouse.delete_profile(n(p)?)?;
                    profile::forget(mouse, n(p)?)?;
                }
                ["switch", p] => mouse.set_active_profile(n(p)?)?,
                ["rename", p, name @ ..] => profile::rename(mouse, n(p)?, &name.join(" "))?,
                _ => return Err("profile [create|delete|switch N | rename N NAME]".into()),
            }
            let p = mouse.profiles()?;
            let names = profile::names(mouse);
            for id in &p.ids {
                let name = names.get(id).map_or(String::new(), |n| format!(" {n}"));
                println!("P{id}{name}{}", if *id == p.active { " [active]" } else { "" });
            }
            println!("{} of {} profiles", p.ids.len(), p.max);
        }
        ["export", path] => {
            profile::export(mouse, o.profile, Path::new(path))?;
            println!("saved profile {} to {path}", o.profile);
        }
        ["import", path] => {
            let file = device::load_json(Path::new(path))?;
            let n = profile::apply(mouse, o.profile, &file)?;
            println!("wrote {n} buttons + DPI to profile {}", o.profile);
        }
        _ => return Err(format!("bad command\n\n{USAGE}").into()),
    }
    Ok(())
}

fn init(args: &[&str], force: bool) -> Result<(), Box<dyn Error>> {
    let connected = hid::razer_devices();
    let known_pids: Vec<u16> = device::definitions().iter().map(|d| d.pid).collect();
    let pid = match args.first() {
        Some(p) => u16::from_str_radix(p.trim_start_matches("0x"), 16).map_err(|_| format!("bad pid '{p}', use hex like 008d"))?,
        None => connected.iter().find(|(p, _)| !known_pids.contains(p)).map(|(p, _)| *p).ok_or_else(|| {
            let list: Vec<String> = connected.iter().map(|(p, n)| format!("{p:04x} {n}")).collect();
            format!("every connected Razer device already has a definition ({}), pass a pid to redo one", list.join(", "))
        })?,
    };
    let hid_name = connected.iter().find(|(p, _)| *p == pid).map_or(format!("Razer 1532:{pid:04x}"), |(_, n)| n.clone());
    let s = scaffold::scaffold(pid, &hid_name)?;
    let path = match args.get(1) {
        Some(p) => PathBuf::from(p),
        None => device::model_dir(0x1532, pid).ok_or("no config dir")?.join("buttons.jsonc"),
    };
    write_new(&path, &s.json, force)?;
    println!(
        "wrote {}\n{hid_name} on interface {}, transaction id 0x{:02x}, {} buttons found\n\
         check every TODO in it, then see https://github.com/{}/blob/main/CONTRIBUTING.md",
        path.display(),
        s.interface,
        s.transaction_id,
        s.buttons,
        protocol::REPO
    );
    Ok(())
}

/// Sends every read command (id 0x80 to 0xff) of each class to a hidraw node and
/// prints the ones that answer, for mice without a device file yet.
fn probe(args: &[&str]) -> Result<(), Box<dyn Error>> {
    let (path, rest) = match args.split_first() {
        Some((p, rest)) if p.starts_with('/') => (Some(*p), rest),
        _ => (None, args),
    };
    let mut classes: Vec<u8> = Vec::new();
    let mut cmd_args: Vec<u8> = Vec::new();
    let mut tid = None;
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match *a {
            "--args" => {
                let hex = it.next().ok_or("--args needs hex bytes, like 004000")?;
                cmd_args = action::hex_bytes(hex).ok_or(format!("bad --args '{hex}', use hex bytes like 004000"))?;
            }
            "--tid" => {
                let hex = it.next().ok_or("--tid needs a transaction id, like 0x3f")?;
                tid = Some(u8::from_str_radix(hex.trim_start_matches("0x"), 16).map_err(|_| format!("bad --tid '{hex}', use hex like 0x3f"))?);
            }
            c => classes.push(parse_byte(c).ok_or(format!("bad class '{c}', use hex like 0x02"))?),
        }
    }
    // a report has 80 argument bytes, from byte 8 up to the checksum
    if cmd_args.len() > 80 {
        return Err("--args takes at most 80 bytes".into());
    }
    if classes.is_empty() {
        classes = (0x00..0x10).collect();
    }

    let hid = match path {
        // the transaction id most newer Razer mice take
        Some(path) => hid::Hid::open(Path::new(path), tid.unwrap_or(0x1f)).map_err(|e| format!("{path}: {e}"))?,
        None => find_probe_target(tid)?,
    };
    let get = |class: u8, id: u8, args: &[u8]| {
        // only GET commands, the ones with bit 0x80 set, so probing can't change the mouse
        if id & 0x80 == 0 {
            return Err(io::Error::other(format!("{class:02x}/{id:02x} isn't a read command")));
        }
        hid.command(class, id, 0x50, args)
    };
    match get(0x00, 0x81, &[]) {
        Ok(r) => println!("firmware: status=ok v{}.{}", r[8], r[9]),
        Err(e) => println!("firmware: {e}"),
    }
    for class in classes {
        for id in 0x80..=0xff {
            let Ok(r) = get(class, id, &cmd_args) else { continue };
            let size = r[5];
            let data = &r[8..(8 + size as usize).min(r.len())];
            let end = data.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
            // the factory serial would end up in protocol files posted publicly
            let data = if (class, id) == (0x00, 0x82) { &[][..] } else { &data[..end] };
            let hex: String = data.iter().map(|b| format!("{b:02x}")).collect();
            println!("class 0x{class:02x} id 0x{id:02x} size={size:3} data={hex}");
        }
    }
    Ok(())
}

/// With no path we find the control interface and transaction id the way `init`
/// does, but only when there's one Razer device, we don't guess between mice.
fn find_probe_target(tid: Option<u8>) -> Result<hid::Hid, Box<dyn Error>> {
    let devices = hid::razer_devices();
    let [(pid, name)] = devices.as_slice() else {
        if devices.is_empty() {
            return Err("no Razer mouse found, is it plugged in?".into());
        }
        let mut list = String::new();
        for (pid, name) in &devices {
            let nodes: Vec<String> = hid::find_all(0x1532, *pid).iter().map(|(_, p)| p.display().to_string()).collect();
            list += &format!("\n  1532:{pid:04x} {name}: {}", nodes.join(" "));
        }
        return Err(format!("more than one Razer device, pass the hidraw node to probe:{list}").into());
    };
    let (hid, iface, found_tid) = scaffold::connect(*pid)?;
    let path = hid::find(0x1532, *pid, iface).ok_or("the mouse went away")?;
    println!("using {} ({name}, 1532:{pid:04x}, interface {iface}, tid 0x{found_tid:02x})", path.display());
    let Some(tid) = tid else { return Ok(hid) };
    Ok(hid::Hid::open(&path, tid)?)
}

/// A byte as hex with 0x (0x02) or decimal (2), like Python's int(x, 0).
fn parse_byte(s: &str) -> Option<u8> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u8::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

/// Like `device::write`, but refuses to overwrite a file unless forced.
fn write_new(path: &Path, contents: &str, force: bool) -> Result<(), Box<dyn Error>> {
    if path.exists() && !force {
        return Err(format!("{} already exists, use --force to overwrite it", path.display()).into());
    }
    Ok(device::write(path, contents)?)
}

/// devices/protocols/<device file>.md inside a checkout of the repo, else ./<device file>.md
fn protocol_path(def: &device::DeviceDef) -> PathBuf {
    let name = format!("{}.md", def.file);
    let dir = Path::new("devices/protocols");
    if dir.is_dir() {
        return dir.join(name);
    }
    PathBuf::from(name)
}

/// Opens the issue with the GitHub CLI, or prints a link to open it by hand.
fn send_issue(path: &Path, title: &str, body: &str, yes: bool) -> Result<(), Box<dyn Error>> {
    let has_gh = Command::new("gh").arg("--version").output().is_ok_and(|out| out.status.success());
    if !has_gh {
        let encode = |s: &str| -> String {
            s.bytes()
                .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
                .collect()
        };
        println!(
            "GitHub CLI (gh) not found. Run `opensynapse protocol send --dry-run`, open this link and paste its output as the issue text:\n\
             https://github.com/{}/issues/new?title={}",
            protocol::REPO,
            encode(title)
        );
        return Ok(());
    }
    if !yes {
        print!(
            "This posts {} and your device file as a public issue on github.com/{} as your GitHub user. Continue? [y/N] ",
            path.display(),
            protocol::REPO
        );
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            return Err("cancelled".into());
        }
    }
    let status = Command::new("gh")
        .args(["issue", "create", "--repo", protocol::REPO, "--title", title, "--body", body])
        .status()?;
    if !status.success() {
        return Err("gh issue create failed (try `gh auth login`)".into());
    }
    Ok(())
}

fn parse_effect(name: &str, args: &[&str]) -> Result<Effect, String> {
    let color = |i: usize| {
        let raw = args.get(i).ok_or(format!("{name} needs a color like ff00aa"))?;
        rgb::parse_color(raw).ok_or(format!("bad color '{raw}'"))
    };
    Ok(match name {
        "off" => Effect::Off,
        "static" => Effect::Static(color(0)?),
        "breath" if args.first() == Some(&"random") => Effect::BreathRandom,
        "breath" => Effect::Breath(color(0)?),
        "spectrum" => Effect::Spectrum,
        "wave" => Effect::Wave(args.first() == Some(&"reverse")),
        "reactive" => Effect::Reactive {
            color: color(0)?,
            speed: args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2),
        },
        _ => return Err(format!("unknown effect '{name}'")),
    })
}

fn main() -> ExitCode {
    // Rust ignores SIGPIPE, so `opensynapse dump | head` would panic instead of exiting quietly like other CLIs
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    match parse().and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("opensynapse: {e}");
            ExitCode::FAILURE
        }
    }
}
