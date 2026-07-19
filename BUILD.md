# Build Instructions

This guide covers how to set up the development environment and build Handy Tool from source.

> The repository vendors its two patched speech dependencies (`transcribe-rs-local/`,
> `whisper-rs-local/` — see [Local dependency forks](#local-dependency-forks)), so a plain
> clone contains everything the build needs.

## Prerequisites

### All Platforms

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager
- [Tauri Prerequisites](https://tauri.app/start/prerequisites/)
- CMake and Ninja (whisper.cpp is built from source)

### Platform-Specific Requirements

#### Windows (primary, tested)

- Visual Studio Build Tools with the C++ workload. **The included `.cmd` scripts expect
  Visual Studio 18 (2026) Build Tools** at the default install path — for VS 2022 (17.x),
  edit the `VCVARS` line at the top of each script, or run `bun run tauri build` from a
  developer prompt instead.
- A [Vulkan SDK](https://vulkan.lunarg.com/) (auto-detected under `C:\VulkanSDK\*`) — Whisper GPU acceleration
- **LLVM/libclang 18.x** for bindgen. LLVM 22+ mis-parses whisper.cpp headers and breaks the
  `whisper-rs-sys` build (`error[E0080]: ... 1_usize - 304_usize`). Point `LIBCLANG_PATH` at an
  LLVM 18 `bin` directory.
- Keep the Cargo build path short: whisper.cpp's Vulkan shader builds exceed the MSVC 250-char
  path limit under deep folders. The included `build.cmd` / `check.cmd` scripts set
  `CARGO_TARGET_DIR=C:\tmp\hb` for this reason.

#### macOS (untested in this fork)

- Xcode Command Line Tools: `xcode-select --install`

#### Linux (untested in this fork)

- Build essentials, ALSA, GTK, WebKit, Vulkan:

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

### 3. Voice-activity-detection model

The Silero VAD model ships in the repository (`src-tauri/resources/models/silero_vad_v4.onnx`).
If it is ever missing, restore it with:

```bash
curl -o src-tauri/resources/models/silero_vad_v4.onnx https://blob.handy.computer/silero_vad_v4.onnx
```

### 4. Start the Dev Server

```bash
bun run tauri dev
# If a cmake policy error appears on macOS:
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev
```

### 5. Release build

```bash
bun run tauri build
```

On Windows you can also use the convenience scripts (they configure the compiler environment,
Vulkan SDK, LLVM 18 libclang, and the short build path automatically):

```bat
check.cmd     :: cargo check + frontend lint (fast gate)
build.cmd     :: full release build (NSIS + MSI installers)
unittest.cmd  :: Rust unit tests against the release artifacts
```

Installers land in `C:\tmp\hb\release\bundle\nsis\` and `...\msi\`.

## Local dependency forks

Two speech dependencies are vendored inside this repository and referenced by path from
`src-tauri/Cargo.toml`:

- **`transcribe-rs-local/`** — fork of [transcribe-rs](https://github.com/cjpais/transcribe-rs)
  updated for whisper-rs 0.15 (API renames, segment iteration).
- **`whisper-rs-local/`** — clone of [whisper-rs](https://codeberg.org/tazz4843/whisper-rs)
  v0.15.1 carrying whisper.cpp v1.8.2+ with Vulkan support and a CMake 4.x compatibility patch.

They build as part of the normal workspace — no extra steps.
