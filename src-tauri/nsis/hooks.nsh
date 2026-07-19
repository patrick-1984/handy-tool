; T-217 — Windows first-launch prerequisites (NSIS installer hooks)
;
; Included by tauri-bundler's generated installer.nsi via
; bundle > windows > nsis > installerHooks in tauri.conf.json.
;
; IMPORTANT: this file is !include'd BEFORE the bundler's own !define block
; (installer.nsi includes installer_hooks around line 29, then !defines ARCH,
; PRODUCTNAME etc. starting around line 49 — verified 2026-07-19 against the
; vendored tauri-bundler installer.nsi template in the cargo git checkout).
; ${ARCH} is therefore NOT usable in top-level !ifdef/!if directives in this
; file (an earlier version of this hook did that and silently compiled to a
; permanent no-op). ${ARCH}/${PRODUCTNAME}/etc. only become safe to branch on
; inside a !macro body, because macro bodies are compiled at !insertmacro
; time (NSIS_HOOK_POSTINSTALL is inserted near the end of Section Install,
; long after all !defines exist) — see the comment on the macro below.
;
; What this file does (NSIS_HOOK_POSTINSTALL):
;   Detect the arch-matched Microsoft Visual C++ 2015-2022 runtime via
;   HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\<arch> (64-bit
;   registry view; "Installed" DWORD = 1). If present: no-op. If missing:
;   inform the user with the official aka.ms download URL and offer to open
;   it in the browser. The install NEVER fails or blocks because of this —
;   silent installs (/S) auto-continue via /SD IDNO, and Tauri passive
;   installs ($PassiveMode, /P) skip the prompt entirely (see macro body).
;
; Why a message box instead of an in-installer download (verified 2026-07-19
; against the tauri-bundler NSIS toolchain in %LOCALAPPDATA%\tauri\NSIS):
;   - The toolchain ships only the stock NSIS plugin set plus
;     nsis_tauri_utils.dll. Neither inetc nor NScurl is present.
;   - nsis_tauri_utils.dll (bundled version) exports only FindProcess/
;     KillProcess(/CurrentUser), RunAsUser and SemverCompare — no download
;     function.
;   - The only stock downloader is NSISdl, which speaks plain HTTP only (no
;     TLS); https://aka.ms/... redirects are HTTPS-only, so NSISdl is not a
;     reliable transport for the redistributable.
;   Per T-217, with no reliable download plugin available we surface a clear
;   message + URL and continue rather than fail the install.
;
; WebView2 is handled separately by the bundler itself: tauri.conf.json sets
; webviewInstallMode = downloadBootstrapper, and the generated installer
; already detects/downloads/installs the Evergreen bootstrapper (and aborts
; visibly if that install fails).

!include LogicLib.nsh

; Arch-matched redistributable selection MUST live inside the macro body, not
; at file top level. tauri-bundler's installer.nsi does:
;   {{#if installer_hooks}} !include "{{installer_hooks}}" {{/if}}   ; ~line 29
;   ...
;   !define ARCH "{{arch}}"                                          ; ~line 49
; i.e. this file is !include'd BEFORE ${ARCH} is !define'd. Preprocessor
; directives (!ifdef/!if) at top level run immediately at include time, so a
; top-level "!ifdef ARCH" here would always see ARCH as undefined and the
; whole hook would silently compile to a permanent no-op regardless of arch.
; Directives *inside* a !macro body, by contrast, are only evaluated when the
; macro is expanded via !insertmacro — which for NSIS_HOOK_POSTINSTALL happens
; near the end of Section Install, long after ARCH is defined. So the arch
; !if/!else chain below is nested inside the macro on purpose.
;
; This fork currently ships x64 only, but the arm64 branch keeps the hook
; correct if an arm64 bundle is ever built. No VCRT_* defines => the whole
; hook compiles to a no-op (never install a wrong-arch redistributable).
!macro NSIS_HOOK_POSTINSTALL
  !if "${ARCH}" == "x64"
    !define VCRT_ARCH "x64"
    !define VCRT_URL "https://aka.ms/vs/17/release/vc_redist.x64.exe"
  !else if "${ARCH}" == "arm64"
    !define VCRT_ARCH "arm64"
    !define VCRT_URL "https://aka.ms/vs/17/release/vc_redist.arm64.exe"
  !endif

  !ifdef VCRT_ARCH
    DetailPrint "Checking for Microsoft Visual C++ 2015-2022 Runtime (${VCRT_ARCH})..."
    ; The canonical detection key lives in the 64-bit registry view; the NSIS
    ; process is 32-bit, so switch views explicitly and restore afterwards.
    SetRegView 64
    ClearErrors
    ReadRegDWORD $0 HKLM "SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\${VCRT_ARCH}" "Installed"
    SetRegView lastused
    ${If} ${Errors}
      StrCpy $0 0
    ${EndIf}
    ${If} $0 = 1
      DetailPrint "Visual C++ Runtime (${VCRT_ARCH}) is already installed."
    ${Else}
      DetailPrint "Visual C++ Runtime (${VCRT_ARCH}) NOT found."
      DetailPrint "Download it from: ${VCRT_URL}"
      ; Never block an unattended install. Two distinct unattended modes exist:
      ;   - Tauri's own passive mode ($PassiveMode, set via the installer's
      ;     /P flag) still renders the installer UI, so a MessageBox would
      ;     pop up and hang a script waiting for it to exit. Skip straight to
      ;     a log line + continue, same effective outcome as a "No" answer.
      ;   - Native NSIS silent mode (/S) is handled by MessageBox's own
      ;     /SD IDNO, which auto-answers "No" (don't open the browser) and
      ;     falls through without ever drawing a window.
      ; Either way the install proceeds; this hook can never fail or abort it.
      ${If} $PassiveMode = 1
        DetailPrint "Passive install: continuing without prompting for the VC++ Runtime."
      ${Else}
        MessageBox MB_YESNO|MB_ICONEXCLAMATION \
          "${PRODUCTNAME} needs the Microsoft Visual C++ 2015-2022 Runtime (${VCRT_ARCH}), which was not found on this computer.$\r$\n$\r$\nWithout it, ${PRODUCTNAME} may fail to start after installation.$\r$\n$\r$\nDownload URL:$\r$\n${VCRT_URL}$\r$\n$\r$\nOpen the download page in your browser now?$\r$\n(Installation will continue either way.)" \
          /SD IDNO IDYES t217_vcrt_open
        Goto t217_vcrt_done
      t217_vcrt_open:
        ExecShell "open" "${VCRT_URL}"
      t217_vcrt_done:
      ${EndIf}
    ${EndIf}
    !undef VCRT_ARCH
    !undef VCRT_URL
  !endif
!macroend
