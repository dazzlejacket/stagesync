#!/usr/bin/env python3
"""Static check of the StageSync NSIS installer script.

`makensis` is the only tool that can really compile an NSIS script, and it is
not installed on every machine that has to review one. This script checks the
mistakes that are worth catching without it:

  * a `${NAME}` reference to a define that is never defined;
  * a `$NAME` reference to a define that *is* defined - the mistake the audit
    found in `InstallDir "$STAGESYNC_INSTALL_DIR"`, where the bare `$` form
    asks for a runtime *variable* instead of the compile-time define, and the
    value silently ends up empty;
  * a path that contains a backslash escape NSIS would resolve - `\\n` is a
    newline and `\\r` a carriage return, so `"$INSTDIR\\notices"` is not a
    directory called "notices";
  * a define that is declared and never used, which is usually a sign that a
    value was meant to be passed but is not.

Usage:

    python packaging/windows/check_nsi.py packaging/windows/installer.nsi
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

# Commands whose arguments are file-system paths. A backslash in one of these
# is an escape sequence, not a separator.
PATH_COMMANDS = [
    "SetOutPath",
    "File",
    "Delete",
    "RMDir",
    "CreateShortcut",
    "CreateDirectory",
    "WriteUninstaller",
    "InstallDir",
    "OutFile",
    "ExecShell",
]

# The escapes NSIS resolves inside a string.
NSIS_ESCAPES = {
    "n": "a newline",
    "r": "a carriage return",
    "t": "a tab",
}

DEFINE = re.compile(r"^\s*!define\s+([A-Za-z_][A-Za-z0-9_]*)", re.M)
BRACED_REFERENCE = re.compile(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}")
BARE_REFERENCE = re.compile(r"\$([A-Za-z_][A-Za-z0-9_]*)")


def strip_comments(text: str) -> str:
    """Remove `;` comments, but not a `;` inside a quoted string."""
    out = []
    for line in text.splitlines():
        result = []
        in_string = False
        index = 0
        while index < len(line):
            char = line[index]
            if char == '"':
                in_string = not in_string
            elif char == ";" and not in_string:
                break
            result.append(char)
            index += 1
        out.append("".join(result))
    return "\n".join(out)


def check(path: pathlib.Path) -> list[str]:
    raw = path.read_text(encoding="utf-8-sig")
    text = strip_comments(raw)
    problems: list[str] = []

    defines = set(DEFINE.findall(text))
    braced = set(BRACED_REFERENCE.findall(text))

    for name in sorted(braced - defines):
        problems.append(
            "line references ${%s}, but %s is never defined" % (name, name)
        )

    # The bare `$NAME` form asks for a runtime variable. When NAME is one of
    # our own defines that is always a mistake: the value would be empty.
    for match in BARE_REFERENCE.finditer(text):
        name = match.group(1)
        if name in defines:
            line = text.count("\n", 0, match.start()) + 1
            problems.append(
                "line %d uses $%s where the define ${%s} was meant: the bare "
                "$ form reads a runtime variable, which is not defined, so the "
                "value ends up empty" % (line, name, name)
            )

    # Backslash escapes inside paths.
    for number, line in enumerate(text.splitlines(), start=1):
        stripped = line.strip()
        command = stripped.split(None, 1)[0] if stripped else ""
        if command not in PATH_COMMANDS:
            continue
        for argument in re.findall(r'"([^"]*)"', line):
            for escape, meaning in NSIS_ESCAPES.items():
                if "\\" + escape in argument:
                    problems.append(
                        "line %d: the path %r contains \\%s, which NSIS resolves "
                        "as %s; write the path with forward slashes"
                        % (number, argument, escape, meaning)
                    )

    # A define that nothing refers to.
    for name in sorted(defines):
        if name.startswith("STAGESYNC_") and name not in braced:
            problems.append(
                "%s is defined but never referenced: it is probably meant to be "
                "passed to the installer but is not" % name
            )

    return problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("script", type=pathlib.Path)
    args = parser.parse_args(argv)

    if not args.script.is_file():
        raise SystemExit("error: %s was not found" % args.script)

    problems = check(args.script)
    if problems:
        print("%s: %d problem(s)" % (args.script, len(problems)))
        for problem in problems:
            print("  - %s" % problem)
        return 1

    print("%s: no problems found" % args.script)
    return 0


if __name__ == "__main__":
    sys.exit(main())
