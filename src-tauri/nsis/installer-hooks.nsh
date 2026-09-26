; NSIS installer hooks, included by tauri-bundler's generated installer.nsi via
; bundle > windows > nsis > installerHooks in tauri.conf.json.
;
; (hooks.nsh next to this file is the retired T-217 VC++ runtime prompt; it has
; not been wired in since the runtime is bundled with the app.)

!include LogicLib.nsh

; People run setup by hand to see what changed, so the launch that follows an
; interactive install or update opens the main window even when Start Hidden is
; on. The installer leaves a one-time marker next to the exe; the app deletes it
; at startup (take_show_window_marker in lib.rs). Silent installs (/S — the
; updater's quiet mode, including scheduled night-time updates) and Tauri's
; passive mode (/P) leave none, so they never pop a window up.
!macro NSIS_HOOK_POSTINSTALL
  ${IfNot} ${Silent}
  ${AndIf} $PassiveMode <> 1
    ClearErrors
    FileOpen $0 "$INSTDIR\show-window-once" w
    ${IfNot} ${Errors}
      FileClose $0
    ${EndIf}
  ${EndIf}
!macroend

; A marker that was never consumed (the app not started after setup) must not
; keep the install folder alive after uninstalling.
!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\show-window-once"
!macroend
