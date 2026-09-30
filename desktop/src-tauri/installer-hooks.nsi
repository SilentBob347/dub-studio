; Installer hooks for the NSIS package.
;
; The executable used to be named after the Rust crate or the product, and is
; now Dub-Studio.exe everywhere (mainBinaryName). The old names are removed so an
; updated installation does not keep a dead copy of the program.

!macro NSIS_HOOK_PREINSTALL
  Delete "$INSTDIR\dub-studio-desktop.exe"
  Delete "$INSTDIR\Dub Studio.exe"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; The studio keeps its models and projects beside the executable, and the
  ; template's "delete application data" only knows about the profile. When it is
  ; ticked, the listed data folders go too. Only these folders are removed, never
  ; $INSTDIR recursively: the program may have been installed into a shared folder
  ; or a drive root.
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    RMDir /r "$INSTDIR\models"
    RMDir /r "$INSTDIR\workspace"
    RMDir /r "$INSTDIR\tools"
    RMDir /r "$INSTDIR\voices"
    RMDir /r "$INSTDIR\casting_library"
    RMDir /r "$INSTDIR\webview-data"
    RMDir /r "$INSTDIR\temp"
  ${EndIf}

  ; Empty-only: an unticked uninstall leaves the weights alone, and the folder
  ; itself disappears once nothing is left in it.
  RMDir "$INSTDIR"
!macroend
