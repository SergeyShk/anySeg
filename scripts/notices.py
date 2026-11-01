"""The licenses of the Rust crates linked into the Python module: THIRD-PARTY-NOTICES.txt

Run: `python scripts/notices.py [path]` from the repository after a change of `Cargo.lock`; without
a path the file of the repository is written. The crates are the normal dependencies of
`anyseg-py` on any target, without proc macros; every one gets its MIT license and the license of
the Unicode data, as the crates offer them.
"""

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).parents[1]
HEADER = [
    "Licenses of the Rust crates linked into the Python module of anySeg. Every crate offers a choice",
    "of licenses; the MIT license is the one used here, with the Unicode license of the Unicode data.",
    "",
]


def linked(metadata: dict) -> list[dict]:
    """The crates of crates.io that the module links on some target, by name"""
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    root = next(
        package["id"] for package in metadata["packages"] if package["name"] == "anyseg-py"
    )
    seen, todo = set(), [root]
    while todo:
        for dependency in nodes[todo.pop()]["deps"]:
            package = packages[dependency["pkg"]]
            normal = any(kind["kind"] is None for kind in dependency["dep_kinds"])
            macro = any("proc-macro" in target["kind"] for target in package["targets"])
            if normal and not macro and package["id"] not in seen:
                seen.add(package["id"])
                todo.append(package["id"])
    crates = [packages[package] for package in seen if packages[package]["source"] is not None]
    return sorted(crates, key=lambda package: package["name"])


def notices() -> str:
    """The text of the file"""
    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--locked"],
            cwd=ROOT,
            capture_output=True,
            check=True,
        ).stdout
    )
    lines = list(HEADER)
    for package in linked(metadata):
        folder = Path(package["manifest_path"]).parent
        texts = [
            path
            for path in sorted(folder.iterdir())
            if path.name.upper().startswith(("LICENSE-MIT", "LICENSE-UNICODE", "COPYRIGHT"))
        ]
        rule = "=" * 100
        title = f"{package['name']} {package['version']} ({package['license']})"
        lines += [rule, f"{title}: {package.get('repository') or ''}", rule, ""]
        for path in texts:
            lines += [path.read_text(encoding="utf-8").strip(), ""]
    text = "\n".join(line.rstrip() for line in "\n".join(lines).splitlines())
    return text.rstrip() + "\n"


if __name__ == "__main__":
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "THIRD-PARTY-NOTICES.txt"
    path.write_text(notices(), encoding="utf-8")
