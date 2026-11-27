; ==============================================================================
; LibOne NSIS Custom Installer & Uninstaller Hooks
;
; Guarantees:
; 1. Original user documents on disk are NEVER modified, moved, or deleted.
; 2. Application uninstallation removes all application binaries and shortcuts.
; 3. Prompts user whether to preserve or remove LibOne's local SQLite metadata
;    (%APPDATA%\com.libone.app), enabling seamless metadata preservation on upgrade
;    or clean removal upon request.
; ==============================================================================

!macro NSIS_HOOK_PREUNINSTALL
  ; Inquire whether user wants to preserve LibOne's metadata database
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Do you want to remove LibOne library metadata and settings stored in AppData (%APPDATA%\com.libone.app)?$\n$\n- Click 'No' to preserve your tags, collections, and notes for future installs.$\n- Click 'Yes' to remove application cache and metadata.$\n$\n(Your original document files on your hard drive will NEVER be touched.)" \
    IDNO keep_libone_appdata IDYES remove_libone_appdata

  remove_libone_appdata:
    DetailPrint "Removing LibOne application data from $APPDATA\com.libone.app"
    RMDir /r "$APPDATA\com.libone.app"
    Goto done_libone_appdata

  keep_libone_appdata:
    DetailPrint "Preserving LibOne library metadata in $APPDATA\com.libone.app"

  done_libone_appdata:
!macroend
