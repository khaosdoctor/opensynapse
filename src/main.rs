use opensynapse::action::Action;
use opensynapse::device::{self, DpiStages, Mouse};
use opensynapse::{input, profile};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;
use std::process::ExitCode;

const USAGE: &str = "\
usage: opensynapse <command> [--profile N] [--layer NAME]

commands:
  list                         connected mice and their buttons
  dump                         every button's action
  get <button>                 one button's action
  set <button> <action>        remap a button
  dpi [X [Y]]                  show or set DPI
  stages [DPI ...] [--active N]  show or set DPI stages (max 5)
  watch                        print buttons as you press them
  export <file.json>           save keymap (all layers) + DPI of --profile
  import <file.json>           write a saved profile to --profile

actions:
  key:[mods+]name   e.g. key:ctrl+1, key:f13, key:shift+a (mods: ctrl shift alt meta rctrl ...)
  mouse:name        left right middle back forward wheelup wheeldown
  turbo:button:rate repeat a mouse button (rate unit unknown, device default 20)
  dpi:up|down, profile:0xNN, disabled, raw:hex

profile defaults to 0 (the live slot), layer to normal.";

struct Opts {
    args: Vec<String>,
    profile: u8,
    layer: String,
    active: Option<u8>,
}

fn parse() -> Result<Opts, String> {
    let mut o = Opts { args: Vec::new(), profile: 0, layer: "normal".into(), active: None };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut val = || it.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--profile" => o.profile = val()?.parse().map_err(|_| "bad --profile")?,
            "--layer" => o.layer = val()?,
            "--active" => o.active = Some(val()?.parse().map_err(|_| "bad --active")?),
            "-h" | "--help" => o.args = vec!["help".into()],
            _ => o.args.push(a),
        }
    }
    Ok(o)
}

fn run(o: Opts) -> Result<(), String> {
    let args: Vec<&str> = o.args.iter().map(String::as_str).collect();
    if matches!(args.first(), None | Some(&"help")) {
        println!("{USAGE}");
        return Ok(());
    }
    let mice = device::detect().map_err(|e| e.to_string())?;
    // ponytail: first mouse only, add --device when someone has two
    let mouse: &Mouse = mice.first().ok_or("no supported Razer mouse found (see devices/*.toml)")?;
    let def = &mouse.def;
    let layer = def.layer(&o.layer).ok_or(format!("unknown layer '{}', have {:?}", o.layer, def.layers))?;
    let button = |name: &str| def.button(name).map(|b| b.id).ok_or(format!("unknown button '{name}'"));
    let e = |e: std::io::Error| e.to_string();

    match args.as_slice() {
        ["list"] => {
            for m in &mice {
                let names: Vec<&str> = m.def.buttons.iter().map(|b| b.name.as_str()).collect();
                println!("{} ({:04x}:{:04x})\n  buttons: {}", m.def.name, m.def.vid, m.def.pid, names.join(" "));
            }
        }
        ["dump"] => {
            for b in &def.buttons {
                println!("{:<12} {}", b.name, mouse.get_button(o.profile, b.id, layer).map_err(e)?);
            }
        }
        ["get", name] => println!("{}", mouse.get_button(o.profile, button(name)?, layer).map_err(e)?),
        ["set", name, action] => {
            let action: Action = action.parse()?;
            mouse.set_button(o.profile, button(name)?, layer, &action).map_err(e)?;
            println!("{name} = {action}");
        }
        ["dpi"] => {
            let (x, y) = mouse.dpi().map_err(e)?;
            println!("{x} x {y}");
        }
        ["dpi", rest @ ..] if rest.len() <= 2 => {
            let x: u16 = rest[0].parse().map_err(|_| "bad DPI")?;
            let y: u16 = rest.get(1).map_or(Ok(x), |v| v.parse()).map_err(|_| "bad DPI")?;
            mouse.set_dpi(x, y).map_err(e)?;
            println!("{x} x {y}");
        }
        ["stages", rest @ ..] => {
            let mut s = mouse.dpi_stages().map_err(e)?;
            if !rest.is_empty() {
                s.stages = rest.iter().map(|v| v.parse()).collect::<Result<_, _>>().map_err(|_| "bad DPI")?;
            }
            if let Some(a) = o.active {
                s.active = a;
            }
            if !rest.is_empty() || o.active.is_some() {
                s.active = s.active.min(s.stages.len() as u8);
                mouse.set_dpi_stages(&s).map_err(e)?;
            }
            print_stages(&s);
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
        ["export", path] => {
            let file = profile::read(mouse, o.profile).map_err(e)?;
            profile::save(&file, Path::new(path)).map_err(e)?;
            println!("saved profile {} to {path}", o.profile);
        }
        ["import", path] => {
            let file = profile::load(Path::new(path)).map_err(e)?;
            let n = profile::apply(mouse, o.profile, &file).map_err(e)?;
            println!("wrote {n} buttons + DPI to profile {}", o.profile);
        }
        _ => return Err(format!("bad command\n\n{USAGE}")),
    }
    Ok(())
}

fn print_stages(s: &DpiStages) {
    for (i, dpi) in s.stages.iter().enumerate() {
        let mark = if i as u8 + 1 == s.active { "*" } else { " " };
        println!("{mark}{} {dpi}", i + 1);
    }
}

fn main() -> ExitCode {
    match parse().and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("opensynapse: {e}");
            ExitCode::FAILURE
        }
    }
}
