# Security policy

## Supported versions

StageSync is at version 0.1.0 and has not been released yet. Security fixes land on the
branch that is currently being developed; there is no long-term support branch yet.

| Version | Supported |
|---|---|
| 0.1.x | yes |
| anything older | no |

## Reporting a vulnerability

Please **do not open a public issue** for a security problem. Use GitHub's private
vulnerability reporting on the Security tab of the repository, which opens a private
advisory that only the maintainers can see.

If private reporting is not available to you, open a public issue that says only "I would
like to report a security problem privately" and nothing more, and wait for a reply.

Please include:

- what the problem is, in enough detail to reproduce it;
- which version and which platform it affects;
- what an attacker could achieve.

You should get an acknowledgement within a week. Fixes are released as soon as they are
ready, and the advisory is published when the fix ships.

## What StageSync does with your files

StageSync is a local application. It does not talk to the network, and it has no
telemetry, no analytics and no update check. There is no account, and nothing is uploaded.

- The recordings you open are read. They are never written to, normalised or truncated.
  The test suite hashes the inputs before and after a full run to prove it.
- The corrected audio, the sync map and the report are written to the folder you choose.
- The desktop application keeps its window size, its last export folder and its
  preferences in a settings file in the platform's configuration directory.
  On Windows that is `%APPDATA%\StageSync`, on macOS
  `~/Library/Application Support/StageSync`, on Linux `~/.config/stagesync`.

## What a report may contain

`report.json` records the path of every file that was read, so that a result can be
traced back to the recording it came from. If you attach a report to a public issue,
replace those paths with the file names alone.

## Dependency advisories

The dependencies and their licences are recorded in [`LICENSES.md`](LICENSES.md).
Advisories against them are handled the same way as any other report: privately first,
then fixed and published.
