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
  installing autotools, build a static libopus from an official *release* tarball (those
  ship a pre-generated `configure`) and point the crate at it with `LIBOPUS_LIB_DIR` and
  `LIBOPUS_STATIC=1`.
- **Bundle target.** `tauri.conf.json` pins `bundle.targets` to `nsis` for Windows. Pass
  `--bundles app` on macOS. The DMG bundler drives Finder through AppleScript and cannot
  run without a GUI session, so it fails over SSH *after* producing a perfectly good `.app`.

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

**Cause.** Rust procedural macros compile to DLLs that `rustc` loads *while
building* — `thiserror_impl.dll`, `schemars_derive.dll`, `proc_macro_hack.dll`
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

**Do NOT run `cargo clean`.** It is the natural reflex and it makes the problem
worse: it discards proc-macro DLLs that Smart App Control has already accepted,
forcing them to be rebuilt with new hashes that are evaluated as unknown all
over again.

**What actually works — and what does not.** The verdict is reputation-based. It
is sometimes transient, in which case retrying with the target directory intact
clears it within a few attempts. **But it can also be persistent**, and the two
cases look identical in the build output. Tell them apart from the *hash suffix*
in the error:

- **Same DLL, same hash on every attempt** → the verdict is against that exact
  cached file. Retrying only reloads the file SAC already rejected; it cannot
  succeed. Stop retrying.
- **Different DLLs or changing hashes** → genuinely transient. Retry.

For the persistent case, a per-crate `cargo clean -p <crate> --release` looks
like the answer — it forces a fresh artifact while leaving already-accepted DLLs
alone. **It does not work, and it makes things worse.** Measured on
2026-08-20: the rebuilt `schemars_derive.dll` was blocked within seconds, and
removing it forced dependents to rebuild *their* proc-macros, taking the count of
blocked DLLs from one to three (`schemars_derive`, `serde_with_macros`,
`phf_macros`) and turning a single clear error into the `can't find crate for
phf / html5ever / kuchikiki / serde_with` cascade. A fresh unsigned proc-macro
gets evaluated as unknown, and when SAC is in a rejecting mood the replacement is
refused too. The per-crate clean is not meaningfully safer than a full one.

When it is persistent, the only things that actually help are **time** (wait for
the cloud verdict to change, hours not minutes) or **building elsewhere** — this
repository has `.github/workflows/windows-nsis-build.yml` for that, though note
the updater signing key is deliberately absent from CI, so a CI-built installer
still has to be signed locally with `tauri signer sign`.

Do not disable Smart App Control to get past this. Turning it off is one-way:
re-enabling requires a Windows reinstall.

`proc-macro-hack` deserves a note because its filename alarms people:
`proc_macro_hack-<hash>.dll` is the deprecated-but-published
[`proc-macro-hack`](https://crates.io/crates/proc-macro-hack) crate, reached via
`phf 0.10` → `cssparser` → `kuchikiki` → `tauri-utils` → `tauri`. It is a
compile-time plugin, it is not a Handy dependency, and being a proc-macro it is
never linked into the shipped binary. Confirm with:
`cargo tree -i proc-macro-hack`.
