#!/usr/bin/env python3
"""Generate release notes from the commits since the previous reachable tag."""

import argparse
import os
from pathlib import Path
import re
import subprocess
from urllib.parse import quote


CONVENTIONAL_COMMIT = re.compile(
    r"^(?P<type>[a-z]+)(?:\((?P<scope>[^)]+)\))?(?P<breaking>!)?:\s*(?P<message>.+)$",
    re.IGNORECASE,
)
CATEGORIES = {
    "breaking": "⚠️ Breaking Changes",
    "feat": "✨ New Features",
    "fix": "🐛 Bug Fixes",
    "perf": "⚡ Performance",
    "refactor": "🔧 Improvements",
    "docs": "📚 Documentation",
    "other": "📦 Other Changes",
}


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()


def previous_tag(commit, tag):
    parents = git("rev-list", "--parents", "-n", "1", commit).split()[1:]
    if not parents:
        return None
    candidates = git("tag", "--merged", parents[0], "--list", "v[0-9]*").splitlines()
    args = ["describe", "--tags", "--abbrev=0", "--match", "v[0-9]*"]
    # Stable releases include changes made throughout the prerelease cycle.
    if "-" not in tag:
        candidates = [candidate for candidate in candidates if "-" not in candidate]
        args.extend(["--exclude", "*-*"])
    if not candidates:
        return None
    return git(*args, parents[0])


def generate_notes(tag, repository, server_url="https://github.com"):
    if git("rev-parse", "--is-shallow-repository") == "true":
        raise RuntimeError("Release notes require full history; use checkout fetch-depth: 0.")
    commit = git("rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}")
    previous = previous_tag(commit, tag)
    revision_range = f"refs/tags/{previous}..{commit}" if previous else commit
    history = git("log", "--no-merges", "--format=%H%x00%s%x00%b%x00", revision_range)
    groups = {key: [] for key in CATEGORIES}
    base_url = f"{server_url.rstrip('/')}/{repository}"

    fields = history.split("\0")
    for index in range(0, len(fields) - 2, 3):
        sha, subject, body = fields[index:index + 3]
        sha = sha.strip()
        category, scope, message = "other", None, subject
        match = CONVENTIONAL_COMMIT.match(subject)
        if match:
            kind = match["type"].lower()
            category = "perf" if kind == "opt" else kind
            if category not in groups:
                category = "other"
            scope, message = match["scope"], match["message"]
        if (match and match["breaking"]) or re.search(
            r"^BREAKING[ -]CHANGE:", body, re.MULTILINE
        ):
            category = "breaking"
        message = message[0].upper() + message[1:]
        prefix = f"**{scope}**: " if scope else ""
        groups[category].append(
            f"- {prefix}{message} ([{sha[:7]}]({base_url}/commit/{sha}))"
        )

    lines = ["## What's Changed", ""]
    if previous:
        lines.extend([
            f"Changes since [{previous}]({base_url}/releases/tag/{quote(previous, safe='')}).",
            "",
        ])
    for category, title in CATEGORIES.items():
        if groups[category]:
            lines.extend([f"### {title}", "", *groups[category], ""])
    if not any(groups.values()):
        lines.extend(["No new non-merge commits in this release.", ""])
    if previous:
        comparison = f"{quote(previous, safe='')}...{quote(tag, safe='')}"
        lines.append(f"**Full Changelog**: [{previous}...{tag}]({base_url}/compare/{comparison})")
    else:
        lines.append(f"**Full Changelog**: [{tag}]({base_url}/commits/{quote(tag, safe='')})")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--repo", required=True, help="GitHub owner/repository")
    parser.add_argument("--output", type=Path, default=Path("release_notes.md"))
    args = parser.parse_args()
    notes = generate_notes(args.tag, args.repo, os.environ.get("GITHUB_SERVER_URL", "https://github.com"))
    args.output.write_text(notes, encoding="utf-8")


if __name__ == "__main__":
    main()
