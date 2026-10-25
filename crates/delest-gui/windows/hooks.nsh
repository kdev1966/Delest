; Uninstalling Delest takes away what it set up outside its folder: the Task
; Scheduler's check and the hooks in the agents (each agent's file is backed up).
; Without this, both would keep pointing at a delest.exe that no longer exists.
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec '"$INSTDIR\delest.exe" timer off'
  nsExec::Exec '"$INSTDIR\delest.exe" remove --yes'
!macroend
