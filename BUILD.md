# Build Instructions

This guide covers how to set up the development environment and build Handy Tool from source.

**Windows x64 is the production target** — built, tested and released. **macOS on Intel is
built and released but experimental**: it compiles and runs, but has not been exercised as a
daily driver. **Linux and Apple Silicon are planned**; no build is produced or downloadable
for either.

## Prerequisites

### All Platforms

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager
- [Tauri Prerequisites](https://tauri.app/start/prerequisites/)

### Platform-Specific Requirements

#### Windows

- Microsoft C++ Build Tools
- Visual Studio 2019/2022 with C++ development tools
- Or Visual Studio Build Tools 2019/2022

#### macOS (Intel — built and released, experimental)

Verified on macOS 14 with an Intel Mac. Requires **macOS 10.15 or newer**: the vendored
whisper.cpp uses `std::filesystem`, which Apple marks unavailable before 10.15.

- Xcode Command Line Tools — `xcode-select --install`
- CMake
- Full Xcode is **not** required. Metal is enabled via `GGML_METAL_EMBED_LIBRARY`, which
  embeds the shader source and compiles it at runtime, so the Metal compiler that ships
  only inside Xcode.app is never invoked at build time.

Two things that will otherwise cost you an afternoon:

- **libopus.** `audiopus_sys` vendors libopus as a bare git checkout with no `configure`,
  so it falls back to `autogen.sh` and fails on a missing `autoreconf`. Rather than
  installing autotools, build a static libopus from an official _release_ tarball (those
  ship a pre-generated `configure`) and point the crate at it with `LIBOPUS_LIB_DIR` and
  `LIBOPUS_STATIC=1`.
- **Bundle target.** `tauri.conf.json` pins `bundle.targets` to `nsis` for Windows. Pass
  `--bundles app` on macOS. The DMG bundler drives Finder through AppleScript and cannot
  run without a GUI session, so it fails over SSH _after_ producing a perfectly good `.app`.

#### Linux (planned — no build is produced or released)

The Linux target has never been built or run. The dependency sets below are prepared for that
work and are unverified.

- Build essentials
- ALSA development libraries
- Install with:

  ```bash
  # Ubuntu/Debian
  sudo apt update
  sudo apt install build-essential libasound2-dev pkg-config libssl-dev libvulkan-dev vulkan-tools glslc libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libgtk-layer-shell0 libgtk-layer-shell-dev patchelf cmake

  # Fedora/RHEL
  sudo dnf groupinstall "Development Tools"
  sudo dnf install alsa-lib-devel pkgconf openssl-devel vulkan-devel \
    gtk3-devel webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel \
    gtk-layer-shell gtk-layer-shell-devel \
    cmake

  # Arch Linux
  sudo pacman -S base-devel alsa-lib pkgconf openssl vulkan-devel \
    gtk3 webkit2gtk-4.1 libappindicator-gtk3 librsvg gtk-layer-shell \
    cmake
  ```

## Setup Instructions

### 1. Clone the Repository

```bash
git clone https://github.com/patrick-1984/handy-tool.git
cd handy-tool
```

### 2. Install Dependencies

```bash
bun install
```

### 3. Start Dev Server

```bash
bun tauri dev
```

## Signed updater release builds

Release builds create signed Tauri updater artifacts. The private key lives outside
version control at `.keys/handy-updater.key`; the path is gitignored, and the key was
generated with a passphrase stored separately in `.keys/handy-updater.password`.
Before running the local production build, supply both through the Tauri signing
environment variables. Never print or echo either secret, copy them into a source
file, or commit them:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw .keys/handy-updater.key
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = (Get-Content -Raw .keys/handy-updater.password).TrimEnd()
try {
  bun run tauri build
} finally {
  Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY, Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD
}
```

Its matching public key is already embedded in `src-tauri/tauri.conf.json`. Back
up both secret files securely: if the private key or its passphrase is lost,
existing installations can never accept another signed update.

Windows produces an NSIS installer only. The MSI target was dropped because the
updater installs NSIS packages silently in place. The Windows updater channel
publishes the generated NSIS setup executable, its signature, and a `latest.json`
manifest to the public GitHub release.

After the signed build, prepare the three release assets locally:

```powershell
bun run generate:updater-manifest
```

The command reads the sole NSIS setup executable and its generated `.sig`,
copies them to `src-tauri/target/release-artifacts/` using the stable
`Handy.Tool_<version>_x64-setup.exe` name, and writes `latest.json` with both
`windows-x86_64-nsis` and compatibility `windows-x86_64` entries. Upload all
three files to the matching `v<version>` GitHub release before publishing it.
Use `--installer <path>` if more than one NSIS installer is present and
`--notes "..."` to set release notes.

To release an installer that has already passed an exact-binary validation, sign
that same file without rebuilding it. The standalone signer writes only the
detached `.sig`, so the installer's bytes and SHA-256 remain unchanged:

```powershell
$env:TAURI_PRIVATE_KEY_PASSWORD = (Get-Content -Raw .keys/handy-updater.password).TrimEnd()
try {
  bun tauri signer sign --private-key-path .keys/handy-updater.key <validated-installer.exe>
  bun run generate:updater-manifest -- --installer <validated-installer.exe>
} finally {
  Remove-Item Env:TAURI_PRIVATE_KEY_PASSWORD
}
```

This updater signature is separate from Windows Authenticode signing. If
Authenticode is added later, sign the executable first, then create the updater
signature from those final bytes and repeat the exact-binary installer test.

## Portable package (Windows)

`portable.cmd` at the repository root assembles the portable ZIP from a completed
release build. It packages the existing binary and resources; it does not build
the application. For what the package contains, see
[docs/portable.md](docs/portable.md).

## Windows: "can't find crate" during a build (Smart App Control)

On a Windows machine with **Smart App Control** enforced, a build can fail with a
cascade of errors that look like a broken dependency tree but are not:

```
error[E0463]: can't find crate for `thiserror`
error: cannot find attribute `error` in this scope     (x174)
error[E0463]: can't find crate for `schemars`
error: could not compile `tauri-utils` (lib)
```

**Cause.** Rust procedural macros compile to DLLs that `rustc` loads _while
building_ — `thiserror_impl.dll`, `schemars_derive.dll`, `proc_macro_hack.dll`
and friends, in `<target>/release/deps/`. They are freshly compiled and
unsigned, so Smart App Control refuses to load them. `rustc` cannot distinguish
"blocked by the OS" from "not there" and reports the crate as missing.

**Confirm it** before changing anything — the block is recorded in the Windows
event log, not in the build output:

```powershell
Get-WinEvent -LogName 'Microsoft-Windows-CodeIntegrity/Operational' -MaxEvents 200 |
  Where-Object { $_.Id -in 3077,3033 } |
  ForEach-Object { if ($_.Message -match 'attempted to load ([^\s]+)') { $matches[1] } } |
  Group-Object | Sort-Object Count -Descending
```

Windows Defender's antivirus is **not** involved and records no detection, so
`Get-MpThreatDetection` looks clean and an antivirus exclusion changes nothing.
Smart App Control is a separate mechanism with no exclusion list.

**What the verdict is actually against.** A single file, identified by its
content — not "unsigned Rust proc-macros" as a class, and not a path. Measured on
2026-08-29: `serde_derive-5f0c21d65f4cceea.dll` built on 08-17 was refused on
every attempt, while the _same crate_ compiled fresh into a throwaway project a
minute later loaded without complaint. So "SAC is blocking my build" and "SAC is
blocking Rust" are different claims, and only the first one is ever true.

**Diagnose with the event log, not the build output.** The build output tells you
a crate is missing; only the event log tells you which file was refused, and when:

```powershell
Get-WinEvent -FilterHashtable @{
    LogName='Microsoft-Windows-CodeIntegrity/Operational'; Id=3077
    StartTime=(Get-Date).AddMinutes(-30) } |
  ForEach-Object { [regex]::Matches($_.Message,
      '\Device\HarddiskVolume\d+(\tmp\hb\[^\s]+?\.(?:dll|exe))') } |
  ForEach-Object { $_.Groups[1].Value } | Group-Object | Sort-Object Count -Descending
```

`os error 4551` appears in the build log **only** when cargo tries to _execute_ a
blocked build script. When `rustc` fails to _load_ a blocked proc-macro you get a
bare `E0463` and no OS error at all, so "no 4551 in the log" does **not** mean SAC
is uninvolved.

**Build with `CARGO_BUILD_JOBS=1` before concluding the machine is unusable.** This is
the single most effective lever found so far, and it points at what SAC is actually
doing. Measured 2026-09-09: a normal parallel build refused the `serde_json` and
`anyhow` build scripts within a minute; the same tree with `CARGO_BUILD_JOBS=1`
compiled straight past both with no `4551` at all. The reputation check is a
per-binary cloud lookup, and a full-throttle cargo build presents hundreds of unknown
binaries at once — enough of those lookups fail or time out that SAC falls back to
deny. One at a time, each gets a real verdict. It is much slower. It is also the
difference between a build and no build.

Note this also explains the "control probe passes but the real build fails"
contradiction: the probe compiles two or three binaries, never enough to saturate the
lookups.

**Clearing one artifact is worth trying. Clearing many is not.** Deleting a single
refused artifact so cargo rebuilds it sometimes produces bytes SAC then accepts —
that worked for `serde_derive` and for the `whisper-rs-sys` build script on 08-29.
It also sometimes does not: the rebuilt `windows_implement` and
`tauri-plugin-global-shortcut` artifacts were refused again the same minute.

Beyond one or two artifacts this stops being a repair and becomes damage, in two
ways that were both measured on 08-29:

- Forcing _every_ build script to recompile (via
  `CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_OPT_LEVEL`, to change their bytes) took the
  refused set from **1 crate to 9 in eight attempts**. This is the same effect the
  2026-08-20 note recorded as "the per-crate clean makes it worse", and it is real —
  though the mechanism is presenting many unknown binaries at once, not the clean
  itself.
- Purging a crate that is _not_ a proc-macro leaves `can't find crate for X` with
  nothing to repair, because the missing artifact is a plain rlib whose build was
  never reached. Six crates ended up in that state and no amount of retrying
  recovered them.

**When a whole artifact generation is refused, move the target directory aside.**
This is the reliable fix and it should be reached for early rather than after an
hour of purging:

```powershell
Rename-Item C:	mp\hb hb-sac-damaged-<date>     # keep it; it is recoverable
```

The next build recreates `C:	mp\hb` from scratch, so every artifact is fresh and
the stale generation that SAC objected to is gone in one step. It costs a full
whisper.cpp rebuild. Budget an hour, run it detached, and watch the log.

**Where a binary lives affects whether it may run.** On 08-29 a freshly compiled
test executable under `%LOCALAPPDATA%\Temp\...` was refused, while the byte-identical
build under `C:	mp\` ran normally. Keep scratch builds and test binaries out of
`%TEMP%`.

**`cargo fmt` can be refused while the toolchain works.** The `cargo-fmt.exe` shim
is a separate binary and was blocked on 08-29 while `cargo` and `rustc` were fine.
Call the real formatter directly:
`& "$env:USERPROFILE\.rustup	oolchains\stable-x86_64-pc-windows-msvcin
ustfmt.exe" --edition 2021 <files>`

**`sac-unblock-loop.ps1`** in the repository root automates the bounded version of
the single-artifact repair: it runs `unittest.cmd`, reads the event log for what was
actually refused, purges the artifact **and both fingerprint spellings**, and
retries. Two things it gets right that a hand-run does not — cargo's `.fingerprint`
directories are named after the **package** (`windows-implement`, hyphens) while the
artifact is named after the **crate** (`windows_implement`, underscores), so
deleting only the DLL leaves cargo believing the unit is fresh and it never
rebuilds; and it stops immediately on any failure that is not a blocked artifact,
so a real compile error is never retried into noise. If it has not converged within
a few attempts, stop it and move the target directory aside instead.

If even a fresh tree is refused, the remaining levers are **time** (the verdict does
change, hours not minutes) or **building elsewhere** — this repository has
`.github/workflows/windows-nsis-build.yml` for that, though note the updater signing
key is deliberately absent from CI, so a CI-built installer still has to be signed
locally with `tauri signer sign`.

Do not disable Smart App Control to get past this. Turning it off is one-way:
re-enabling requires a Windows reinstall.

`proc-macro-hack` deserves a note because its filename alarms people:
`proc_macro_hack-<hash>.dll` is the deprecated-but-published
[`proc-macro-hack`](https://crates.io/crates/proc-macro-hack) crate, reached via
`phf 0.10` → `cssparser` → `kuchikiki` → `tauri-utils` → `tauri`. It is a
compile-time plugin, it is not a Handy dependency, and being a proc-macro it is
never linked into the shipped binary. Confirm with:
`cargo tree -i proc-macro-hack`.
