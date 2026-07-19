@echo off
setlocal EnableDelayedExpansion

:: ============================================================
:: Handy - Production Build Script
:: ============================================================
:: Prerequisites:
::   - Rust (rustup.rs, stable MSVC toolchain)
::   - Bun (bun.sh)
::   - Visual Studio Build Tools 2025 with "Desktop development with C++"
::   - LLVM 18.x (NOT 22+, bindgen 0.69.x is incompatible)
::   - Vulkan SDK (vulkan.lunarg.com)
::   - CMake (cmake.org or via VS Build Tools)
::   - Model file: src-tauri/resources/models/silero_vad_v4.onnx
:: ============================================================

:: Navigate to project root
cd /d "%~dp0"

:: Load Visual Studio environment
set "VCVARS=C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if not exist "!VCVARS!" (
    echo ERROR: Visual Studio Build Tools not found.
    echo Install "Desktop development with C++" from Visual Studio Installer.
    exit /b 1
)
call "!VCVARS!" >nul 2>&1

:: Set Vulkan SDK (auto-detect if not set)
if "!VULKAN_SDK!"=="" (
    for /d %%V in (C:\VulkanSDK\*) do set "VULKAN_SDK=%%V"
)
if "!VULKAN_SDK!"=="" (
    echo ERROR: Vulkan SDK not found. Install from https://vulkan.lunarg.com/sdk/home
    exit /b 1
)
echo Using VULKAN_SDK=!VULKAN_SDK!

:: Set LLVM/libclang path
if "!LIBCLANG_PATH!"=="" set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"
if not exist "!LIBCLANG_PATH!\libclang.dll" (
    echo ERROR: libclang.dll not found at !LIBCLANG_PATH!
    echo Install LLVM 18.x from https://github.com/llvm/llvm-project/releases/tag/llvmorg-18.1.8
    exit /b 1
)
echo Using LIBCLANG_PATH=!LIBCLANG_PATH!

:: Use Ninja generator to avoid VS version detection issues with cmake crate
set CMAKE_GENERATOR=Ninja

:: T-210: pin a portable CPU baseline for the distributable binary. Without
:: this, ggml defaults GGML_NATIVE=ON and bakes THIS machine's CPU features
:: (via MSVC FindSIMD host detection) into the static whisper.cpp lib, which
:: can illegal-instruction-crash on older supported CPUs before Vulkan
:: init even runs. whisper-rs-sys/build.rs already hardcodes
:: GGML_NATIVE=OFF as its own default, but it also forwards any GGML_* env
:: var (last -D wins in CMake) — so set it explicitly here too, both as
:: defense-in-depth against that default ever changing and to keep this
:: local production script and release CI (.github/workflows/build.yml)
:: on the same explicit policy. Vulkan (GGML_VULKAN) is untouched.
:: Opt back into a machine-tuned build: set GGML_NATIVE=ON before running.
if "!GGML_NATIVE!"=="" set GGML_NATIVE=OFF
echo Using GGML_NATIVE=!GGML_NATIVE! (portable CPU baseline; set GGML_NATIVE=ON to opt out)

:: Check model file exists
if not exist "src-tauri\resources\models\silero_vad_v4.onnx" (
    echo WARNING: VAD model not found. Downloading...
    mkdir "src-tauri\resources\models" 2>nul
    curl -o "src-tauri\resources\models\silero_vad_v4.onnx" https://blob.handy.computer/silero_vad_v4.onnx
)

:: Debug mode disabled in production
set HANDY_DEBUG=false

echo.
echo Building Handy...
echo.
bun run tauri build

if !errorlevel! neq 0 (
    echo.
    echo BUILD FAILED. Check errors above.
    exit /b 1
)

echo.
echo BUILD SUCCEEDED
echo Binary: src-tauri\target\release\handy.exe
