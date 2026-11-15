K Shell 0.1.2 adds right-to-left setup loading, a K Shell ASCII welcome and five
optional prompt designs: Powerline, Atomic, Bubblesline, Cert and Negligible.
Fresh installations use the original default prompt. Type `theme`, preview with
Up/Down and press Enter to install and save a style; Esc or Ctrl+C cancels.
Themes install offline, and the selected design returns in new sessions.

Install K Shell on Windows 10/11 x64 from PowerShell:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/chaovanthoung168/k-shell/main/install.ps1 | iex"
```

Restart your terminal app, then run `kshell`, or select the **K Shell** Windows Terminal profile.

The installer verifies the ZIP's SHA256 checksum, installs for your account, adds the installation folder to your user PATH, and registers a Windows Terminal profile. Administrator access, Rust, WSL, and new fonts are not required. The execution-policy option applies only to the installer process.

For a portable session, download the ZIP and matching `.sha256`, compare with `Get-FileHash`, extract the entire archive, and run `kshell.exe`. To install manually, run the included `Install.ps1 -PathScope User` from PowerShell.

This is an unsigned early Windows x64 release. K Shell supports single commands, native Windows applications, history, completion, themes, and Linux-style utilities. Pipes, shell redirection, variable expansion, and full scripting are still under development. External developer tools are installed separately.
