use opensynapse::action::Action;
use opensynapse::device;
use opensynapse::rgb::{self, Effect};
use opensynapse::generators::protocol;
use opensynapse::{hid, input, profile, scaffold};
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

profile defaults to 0 (the live slot), layer to normal, device to the first mouse in `list`
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
            let light =
                rgb::find(def.vid, def.pid, &mouse.serial).ok_or("no lighting found (is the OpenRazer driver loaded?)")?;
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
    // Rust ignores SIGPIPE, so `opensynapse dump | head` panicked; exit quietly like other CLIs
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    match parse().and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("opensynapse: {e}");
            ExitCode::FAILURE
        }
    }
}
