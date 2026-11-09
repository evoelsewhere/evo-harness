; NSIS installer hooks (tauri.conf.json bundle.windows.nsis.installerHooks).
;
; Tauri's template only stops the main binary before it copies or deletes
; files. Every chat runs its own `evo-engine.exe` from the install folder, and
; an engine left running (an older build's orphan, or one still finishing a
; run) keeps the file locked: "Error opening file for writing: ...evo-engine.exe".
; So before installing or uninstalling, ask to close the app as the template
; does, then stop the app and every engine started from $INSTDIR. The
; template's own check that follows then finds nothing running.

!macro EVO_STOP_APP_AND_ENGINES
  ; Is the app itself running? Ask before closing it, unless silent/passive.
  !insertmacro RestartManager_StartSession $R0
  ${If} $R0 != ""
    !insertmacro RestartManager_RegisterFile $R0 "$INSTDIR\${MAINBINARYNAME}.exe"
    ${If} $0 = 0
      System::Call 'RSTRTMGR::RmGetList(p R0, *i .r1, *i .r2, p 0, *i .r3) i .r0'
      ${If} $0 = ${ERROR_MORE_DATA}
      ${AndIfNot} ${Silent}
      ${AndIf} $PassiveMode != 1
        nsis_tauri_utils::StrReplace "$(appRunningOkKill)" "{{product_name}}" "${PRODUCTNAME}"
        Pop $R2
        ${IfNot} ${Cmd} `MessageBox MB_OKCANCEL "$R2" IDOK`
          nsis_tauri_utils::StrReplace "$(appRunning)" "{{product_name}}" "${PRODUCTNAME}"
          Pop $R1
          !insertmacro RestartManager_EndSession $R0
          Abort $R1
        ${EndIf}
      ${EndIf}
    ${EndIf}
    !insertmacro RestartManager_EndSession $R0
  ${EndIf}

  ; The app first, so it cannot start another engine; then the engines.
  !insertmacro RestartManager_ShutdownFile "$INSTDIR\${MAINBINARYNAME}.exe" $R9
  !insertmacro RestartManager_ShutdownFile "$INSTDIR\evo-engine.exe" $R9
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro EVO_STOP_APP_AND_ENGINES
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro EVO_STOP_APP_AND_ENGINES
!macroend
