# Build Instructions

This guide covers how to set up the development environment and build Handy Tool from source.

**Only the Windows x64 build is produced, tested and released.** The macOS and Linux sections
below cover source targets that are planned and not shipped. Building on them is unsupported and
unverified: no macOS or Linux build has been produced, and no download exists for either.

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

#### macOS (planned — no build is produced or released)

The macOS target has never been built or run. Treat the following as a starting point, not a
supported path.

- Xcode Command Line Tools
- Install with: `xcode-select --install`

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
generated without a passphrase. Before running the local production build, supply it
through the `TAURI_SIGNING_PRIVATE_KEY` environment variable. Never print or echo the
private key, copy it into a source file, or commit it:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw .keys/handy-updater.key
bun run tauri build
Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY
```

The updater key has no passphrase, so `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is
not required. Its matching public key is already embedded in
`src-tauri/tauri.conf.json`. Back up `.keys/handy-updater.key` securely: if it is
lost, existing installations can never accept another signed update.

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

## Portable package (Windows)

`portable.cmd` at the repository root assembles the portable ZIP from a completed
release build. It packages the existing binary and resources; it does not build
the application. For what the package contains, see
[docs/portable.md](docs/portable.md).
