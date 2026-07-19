@echo off
setlocal EnableDelayedExpansion

:: ============================================================
:: Handy - Clean Rebuild Script
:: ============================================================
:: Same as build.cmd but runs cargo clean first to force
:: a full recompilation from scratch.
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

:: Short build path: whisper.cpp Vulkan shader builds exceed the MSVC 250-char
:: path limit under deep folders.
if "!CARGO_TARGET_DIR!"=="" set "CARGO_TARGET_DIR=C:\tmp\hb"
echo Using CARGO_TARGET_DIR=!CARGO_TARGET_DIR!

:: Check model file exists
if not exist "src-tauri\resources\models\silero_vad_v4.onnx" (
    echo WARNING: VAD model not found. Downloading...
    mkdir "src-tauri\resources\models" 2>nul
    curl -o "src-tauri\resources\models\silero_vad_v4.onnx" https://blob.handy.computer/silero_vad_v4.onnx
)

:: Debug mode disabled in production
set HANDY_DEBUG=false

:: Clean build artifacts
echo.
echo Cleaning build artifacts...
cd src-tauri
cargo clean
cd ..

echo.
echo Rebuilding Handy from scratch...
echo.
bun run tauri build

if !errorlevel! neq 0 (
    echo.
    echo BUILD FAILED. Check errors above.
    exit /b 1
)

echo.
echo BUILD SUCCEEDED
echo Binary:     !CARGO_TARGET_DIR!\release\handy.exe
echo Installers: !CARGO_TARGET_DIR!\release\bundle\nsis\ and ...\msi\
