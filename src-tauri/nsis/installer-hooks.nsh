; NSIS installer hooks, included by tauri-bundler's generated installer.nsi via
; bundle > windows > nsis > installerHooks in tauri.conf.json.
;
; (hooks.nsh next to this file is the retired T-217 VC++ runtime prompt; it has
; not been wired in since the runtime is bundled with the app.)

!include LogicLib.nsh

; One line in {app log dir}\installer.log, so a failed update leaves evidence of
; how far it got (a silent install shows nothing). Registers are preserved.
!macro HandyInstallerLog text
  Push $0
  Push $1
  Push $2
  Push $3
  Push $4
  Push $5
  Push $6
  Push $7
  CreateDirectory "$LOCALAPPDATA\${BUNDLEID}\logs"
  ClearErrors
  FileOpen $0 "$LOCALAPPDATA\${BUNDLEID}\logs\installer.log" a
  ${IfNot} ${Errors}
    FileSeek $0 0 END
    ${GetTime} "" "L" $1 $2 $3 $4 $5 $6 $7
    FileWrite $0 "[$3-$2-$1 $5:$6:$7] ${VERSION}: ${text}$\r$\n"
    FileClose $0
  ${EndIf}
  ClearErrors
  Pop $7
  Pop $6
  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
!macroend

; An update the app starts itself (/UPDATE) begins while the app is still
; closing. Every silent update on record made with a speech model loaded failed
; without a word (the app stayed on the old version and did not restart), and
; every update made right after a restart worked. Leading hypothesis: the old
; process was still exiting when the template's app check ran - that check can
; find the app and then fail to kill it once it has exited, and a file still in
; use fails the copy; either ends a silent install. So first wait until the
; installed handy.exe can be opened for writing (append mode: nothing is
; changed), which fails while a process runs it - polled every 250 ms, about a
; minute in all. This only probes handy.exe, not every file the install
; replaces. The outcome is logged; after a timeout the template's own check runs
; as before. Only /UPDATE: a setup started by hand asks the user to close the
; app instead.
!macro NSIS_HOOK_PREINSTALL
  ${If} $UpdateMode = 1
  ${AndIf} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    Push $R8
    Push $R9
    StrCpy $R9 0
    handy_wait_for_exit:
      ClearErrors
      FileOpen $R8 "$INSTDIR\${MAINBINARYNAME}.exe" a
      ${IfNot} ${Errors}
        FileClose $R8
        Goto handy_wait_done
      ${EndIf}
      IntOp $R9 $R9 + 1
      ${If} $R9 < 240
        Sleep 250
        Goto handy_wait_for_exit
      ${EndIf}
      !insertmacro HandyInstallerLog "update: $INSTDIR\${MAINBINARYNAME}.exe could not be opened for writing for about 60 s; continuing to the app check"
      Goto handy_wait_end
    handy_wait_done:
      IntOp $R8 $R9 * 250
      !insertmacro HandyInstallerLog "update: $INSTDIR\${MAINBINARYNAME}.exe opened for writing after about $R8 ms"
    handy_wait_end:
    ClearErrors
    Pop $R9
    Pop $R8
  ${EndIf}
!macroend

; People run setup by hand to see what changed, so the launch that follows an
; interactive install or update opens the main window even when Start Hidden is
; on. The installer leaves a one-time marker next to the exe; the app deletes it
; at startup (take_show_window_marker in lib.rs). Silent installs (/S — the
; updater's quiet mode, including scheduled night-time updates) and Tauri's
; passive mode (/P) leave none, so they never pop a window up.
!macro NSIS_HOOK_POSTINSTALL
  ${If} $UpdateMode = 1
    !insertmacro HandyInstallerLog "update: files installed"
  ${EndIf}
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
