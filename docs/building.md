# Building raiox

Most people should use the installers on the
[Releases](https://github.com/Leooleo1/ai-ray-x/releases) page. Build from source
for Intel Macs, Linux, or to work on raiox.

## Requirements

- Rust 1.88 or newer, and Node.js with pnpm, to build from source.
- Any of Claude Code, Codex, Cursor's `cursor-agent`, Antigravity's `agy`
  (signed in) or opencode (its free models need no account), for the chats.
  The x-ray reads Claude Code and Codex logs.
- Optional: [ai-memory](https://github.com/akitaonrails/ai-memory) for the team
  notes and the messages between projects, and the GitHub CLI (`gh`) to find
  and clone your repositories during setup.

## Run it from source

```sh
git clone https://github.com/Leooleo1/ai-ray-x raiox
cd raiox/apps/web && pnpm install && pnpm build
cd ../.. && cargo build --release
./target/release/raiox open
```

The first run walks you through connecting your AIs, GitHub and projects, and
making a bot for each project.

The binary serves the panel from `apps/web/dist`, so run it from the checkout (or
pass `--web-dir`).

## Desktop app (macOS and Windows)

`crates/desktop` is raiox as an app ([Tauri 2](https://v2.tauri.app)): the same
panel in a window of its own, an icon in the menu bar with how many AIs are
working, and a notification when one finishes while you look elsewhere.
Closing the window keeps raiox in the menu bar; the menu opens it again, opens
it in the browser, turns on opening at login, and quits (which also stops the
dev servers and the tunnel raiox started). The panel server runs inside the
app, so phone access keeps working. One raiox serves a data folder at a time:
the app and `raiox open` refuse to run on the same data at once.

```sh
pnpm --dir crates/desktop install
CI=true pnpm --dir crates/desktop build   # target/release/bundle/macos/raiox.app and dmg/
```

The build is not signed yet: on another Mac, open it the first time with a
right click › Open. (`CI=true` skips the Finder step that arranges the dmg
window.)

On Windows the app lives in the notification area instead of the menu bar,
and installs per user (no administrator rights) with the NSIS installer, or
with the MSI. Building it there needs Rust (rustup), Visual Studio Build Tools
with the C++ workload, Node.js with pnpm and, on ARM64 Windows, LLVM (`clang`,
for the `ring` crate):

```powershell
pnpm --dir crates/desktop install
pnpm --dir crates/desktop build    # target\release\bundle\nsis\*-setup.exe and msi\*.msi
# an Intel/AMD build from ARM64 Windows: add --target x86_64-pc-windows-msvc
```

The Windows build is not signed yet either: SmartScreen asks once (More info ›
Run anyway). In the panel, each CLI's install line is the Windows one there
(PowerShell or npm).
