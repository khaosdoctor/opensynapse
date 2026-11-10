#!/usr/bin/env python3
"""Turns a `opensynapse protocol send` issue into device files.

Reads the issue from a GitHub event JSON file and writes
devices/<slug>.jsonc, devices/protocols/<slug>.md and the BUILTIN entry in
src/device.rs. The issue body is untrusted text: it is only parsed and written
to those files, never run. Prints `slug=<slug>` and `name=<name>` for
$GITHUB_OUTPUT. Usage: device_from_issue.py <event.json>
"""
import json
import re
import sys
from pathlib import Path

HEADER = re.compile(r"^## Device file \(`devices/([a-z0-9-]+)\.jsonc`\)\s*$", re.M)
BLOCK = re.compile(r"```jsonc\n(.*?)\n```", re.S)
NAME = re.compile(r'"name"\s*:\s*"([^"\n]{1,80})"')


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    body = json.loads(Path(sys.argv[1]).read_text())["issue"]["body"] or ""
    body = body.replace("\r\n", "\n")

    header = HEADER.search(body)
    if not header:
        sys.exit("no '## Device file (`devices/<slug>.jsonc`)' heading, was this sent with `opensynapse protocol send`?")
    slug = header.group(1)
    block = BLOCK.search(body, header.end())
    if not block:
        sys.exit("no ```jsonc block after the device file heading")
    device = block.group(1).rstrip() + "\n"
    name = NAME.search(device)
    if not name:
        sys.exit('the device file has no "name"')
    # everything before the device file heading is the protocol file
    protocol = body[: header.start()].rstrip() + "\n"

    Path(f"devices/{slug}.jsonc").write_text(device)
    Path(f"devices/protocols/{slug}.md").write_text(protocol)

    rs = Path("src/device.rs")
    src = rs.read_text()
    entry = f'    ("{slug}", include_str!("../devices/{slug}.jsonc")),\n'
    if entry not in src:
        start = src.index("const BUILTIN")
        end = src.index("];", start)
        src = src[:end] + entry + src[end:]
        rs.write_text(src)

    print(f"slug={slug}")
    print(f"name={name.group(1)}")


if __name__ == "__main__":
    main()
