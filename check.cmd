@echo off
setlocal EnableDelayedExpansion

:: ============================================================
:: Handy - Quick Check Script
:: ============================================================
:: Runs cargo check (type-checking only, no linking) and
:: bun lint to verify code compiles without errors.
:: Much faster than a full build.
:: ============================================================

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
:: Suppress cmake policy warnings for older cmake_minimum_required in whisper.cpp
set CMAKE_POLICY_VERSION_MINIMUM=3.5

:: Debug mode for check
set HANDY_DEBUG=true

:: Use short target dir to avoid MSVC 250-char path limit with whisper.cpp Vulkan shaders
set CARGO_TARGET_DIR=C:\tmp\hb

echo.
echo === Rust cargo check ===
echo.
cd src-tauri
::cargo update
cargo check
set "RUST_RESULT=!errorlevel!"
cd ..

echo.
echo === Frontend lint ===
echo.
bun run lint
set "LINT_RESULT=!errorlevel!"

echo.
echo ========================================
if !RUST_RESULT! neq 0 (
    echo RUST CHECK FAILED
) else (
    echo Rust check: OK
)
if !LINT_RESULT! neq 0 (
    echo FRONTEND LINT FAILED
) else (
    echo Frontend lint: OK
)
echo ========================================

if !RUST_RESULT! neq 0 exit /b 1
if !LINT_RESULT! neq 0 exit /b 1
exit /b 0
