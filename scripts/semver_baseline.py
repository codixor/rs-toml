"""Print the published rs-toml version that CI should use as its API baseline."""
import json
import os
from pathlib import Path
import tomllib
from urllib.request import Request, urlopen


def precedence(version):
    """SemVer precedence for a version already validated by Cargo or crates.io."""
    core, _, pre = version.split("+", 1)[0].partition("-")
    identifiers = tuple((0, int(part)) if part.isdigit() else (1, part)
                        for part in pre.split(".")) if pre else ()
    return tuple(map(int, core.split("."))), not pre, identifiers


def select_baseline(releases, version, *, tagged):
    limit = precedence(version)
    candidates = []
    for release in releases:
        if release["yanked"]:
            continue
        candidate = release["vers"]
        key = precedence(candidate)
        if key < limit or (not tagged and key == limit):
            candidates.append((key, candidate))
    if not candidates:
        raise ValueError(f"no eligible published rs-toml baseline for {version}")
    return max(candidates)[1]


if __name__ == "__main__":
    with Path("Cargo.toml").open("rb") as manifest:
        version = tomllib.load(manifest)["workspace"]["package"]["version"]
    request = Request("https://index.crates.io/rs/-t/rs-toml",
                      headers={"User-Agent": "codixor-rs-toml-ci", "Cache-Control": "no-cache"})
    with urlopen(request, timeout=30) as response:
        releases = [json.loads(line) for line in response]
    print(select_baseline(releases, version, tagged=os.environ["GITHUB_REF_TYPE"] == "tag"))
