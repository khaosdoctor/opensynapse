//! Turns an `opensynapse protocol send` issue into device files, for add-device.yml.
//!
//! Reads the issue from a GitHub event JSON file and writes devices/<slug>.jsonc and
//! devices/protocols/<slug>.md. The issue body is untrusted text: it is only parsed
//! and written to those two files, never run. Prints `slug=<slug>` and `name=<name>`
//! for $GITHUB_OUTPUT.

use opensynapse::device;
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: device-from-issue <event.json>";
const HEADING_START: &str = "## Device file (`devices/";
const HEADING_END: &str = ".jsonc`)";
const BLOCK_START: &str = "```jsonc\n";
const BLOCK_END: &str = "\n```";

struct Issue {
    slug: String,
    name: String,
    device: String,
    protocol: String,
}

fn parse_body(body: &str) -> Result<Issue, String> {
    let body = body.replace("\r\n", "\n");
    let (slug, start, end) = find_heading(&body).ok_or(
        "no '## Device file (`devices/<slug>.jsonc`)' heading, was this sent with `opensynapse protocol send`?",
    )?;
    let block = body[end..].find(BLOCK_START).map(|i| end + i + BLOCK_START.len());
    let close = block.and_then(|b| body[b..].find(BLOCK_END).map(|i| b + i));
    let (Some(block), Some(close)) = (block, close) else {
        return Err("no ```jsonc block after the device file heading".into());
    };
    let device = format!("{}\n", body[block..close].trim_end());
    let def = device::parse(&device).map_err(|e| format!("devices/{slug}.jsonc doesn't parse: {e}"))?;
    // the name ends up in $GITHUB_OUTPUT, where a newline would add an output of its own
    let name = def.name.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err("the device file needs a \"name\" of 1 to 80 characters on one line".into());
    }
    // everything before the device file heading is the protocol file
    let protocol = format!("{}\n", body[..start].trim_end());
    Ok(Issue { slug, name: name.to_string(), device, protocol })
}

/// The slug and the byte range of the first `## Device file (`devices/<slug>.jsonc`)` line.
fn find_heading(body: &str) -> Option<(String, usize, usize)> {
    let mut start = 0;
    for line in body.split_inclusive('\n') {
        let end = start + line.len();
        let slug = line.trim_end().strip_prefix(HEADING_START).and_then(|l| l.strip_suffix(HEADING_END));
        if let Some(slug) = slug.filter(|s| valid_slug(s)) {
            return Some((slug.to_string(), start, end));
        }
        start = end;
    }
    None
}

/// Only `[a-z0-9-]+`, so the slug can't climb out of devices/ when it becomes a path.
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty() && slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn run(event_path: &str) -> Result<Issue, String> {
    let text = std::fs::read_to_string(event_path).map_err(|e| format!("{event_path}: {e}"))?;
    let event: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{event_path}: {e}"))?;
    let issue = parse_body(event["issue"]["body"].as_str().unwrap_or_default())?;
    let device_path = format!("devices/{}.jsonc", issue.slug);
    let protocol_path = format!("devices/protocols/{}.md", issue.slug);
    device::write(Path::new(&device_path), &issue.device).map_err(|e| format!("{device_path}: {e}"))?;
    device::write(Path::new(&protocol_path), &issue.protocol).map_err(|e| format!("{protocol_path}: {e}"))?;
    Ok(issue)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [event_path] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    match run(event_path) {
        Ok(issue) => {
            println!("slug={}", issue.slug);
            println!("name={}", issue.name);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("device-from-issue: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(device: &str) -> String {
        format!(
            "# Razer Test Mouse\r\n\r\nWhat we found.\r\n\r\n## Device file (`devices/razer-test-mouse.jsonc`)\r\n\r\n```jsonc\r\n{device}\r\n```\r\n"
        )
    }

    #[test]
    fn parses_an_issue() {
        let device = r#"// Verified on hardware
{ "name": "Razer Test Mouse", "vid": "0x1532", "pid": "0x0001", "interface": 0, "transaction_id": "0x1f",
  "profiles": [0], "layers": ["normal"], "dpi_max": 100, "buttons": [{ "id": "0x01", "name": "left" }] }"#;
        let issue = parse_body(&body(device)).unwrap();
        assert_eq!((issue.slug.as_str(), issue.name.as_str()), ("razer-test-mouse", "Razer Test Mouse"));
        assert_eq!(issue.device, format!("{}\n", device.replace("\r\n", "\n")));
        assert_eq!(issue.protocol, "# Razer Test Mouse\n\nWhat we found.\n");

        // no pid, so the device parser refuses it and nothing gets written
        let missing_pid = r#"{ "name": "Razer Test Mouse", "vid": "0x1532" }"#;
        let err = parse_body(&body(missing_pid)).err().unwrap();
        assert!(err.contains("doesn't parse") && err.contains("pid"), "{err}");
        assert!(parse_body("## Device file (`devices/../evil.jsonc`)\n```jsonc\n{}\n```").is_err());
    }

    /// The other side of the contract with `protocol send`: what it posts parses back to the same files.
    // fake OpenRazer devices are the only way to get a Mouse outside the library, and they only exist in debug builds
    #[cfg(debug_assertions)]
    #[test]
    fn parses_what_protocol_send_posts() {
        use opensynapse::generators::protocol::{generate, issue};
        let dir = std::env::temp_dir().join(format!("opensynapse-issue-test-{}", std::process::id()));
        let naga = dir.join("0003:1532:008D.0001");
        std::fs::create_dir_all(&naga).unwrap();
        std::fs::write(naga.join("device_type"), "Razer Naga Left-Handed 2020\n").unwrap();
        let mice = opensynapse::fake::mice(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        let mouse = mice.first().expect("the naga from devices/");
        let protocol = generate(mouse);
        let (title, body) = issue(&protocol, mouse);
        assert!(title.starts_with("New Device: "), "{title}");
        let parsed = parse_body(&body).unwrap();
        assert_eq!(parsed.slug, mouse.def.file);
        assert_eq!(parsed.name, mouse.def.name);
        assert_eq!(parsed.device, format!("{}\n", mouse.def.source.trim_end()));
        assert_eq!(parsed.protocol, format!("{}\n", protocol.trim_end()));
    }
}
