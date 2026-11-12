K Shell for Windows x64
======================

Windows 10/11 x64. Rust and developer tools are NOT needed to run this package.

QUICK START (PORTABLE)
1. Extract the entire ZIP to a folder.
2. Double-click kshell.exe, or run .\kshell.exe from PowerShell.
3. Type help to list commands. Type exit to leave K Shell.

INSTALL FOR YOUR ACCOUNT (NO ADMIN REQUIRED)
1. Open PowerShell in the extracted folder.
2. Run:
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Install.ps1
3. Close and reopen Windows Terminal.
4. Select K Shell from Terminal's profile dropdown, or run:
   wt -p "K Shell"

The execution-policy flag above applies only to that PowerShell process; it does
not change your saved policy. Run it only for a package you intend to install.

Installation folder: %LOCALAPPDATA%\Programs\KShell
Terminal fragment: %LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments\K Shell
Your Terminal default profile, other profiles, PATH, and fonts are not changed.
The installed profile points to the installed copy, so you can remove the
extracted download folder after installation. A prior installation is retained
as a kshell-previous-*.exe backup during upgrades. Close old sessions when ready.

WINDOWS TERMINAL / FONTS
Windows Terminal is optional for portable operation and required for its themed
profile. The installer uses MesloLGM Nerd Font Mono when already installed, or
Consolas with standard Unicode icons. No font or other software is downloaded.
To choose an installed Nerd Font Mono explicitly:
   .\Install.ps1 -FontFace 'Your Nerd Font Mono'
Use -NoTerminalProfile to install only the application.
If the profile is missing, restart all Terminal windows. You can also add an
empty profile in Terminal Settings and select the installed kshell.exe, using:
   kshell.exe --theme midnight --icons unicode --suggestions soft

TRY THESE COMMANDS INSIDE K SHELL (ONE LINE AT A TIME)
   pwd
   ls
   tree -L 2
   sysinfo
   mkdir demo
   touch demo/notes.txt
   write demo/notes.txt 'Hello from K Shell'
   append demo/notes.txt 'Another line'
   cat demo/notes.txt
   curl --version

AVAILABILITY
K Shell provides its own built-ins and launches Windows programs natively.
Git, Node/npm, VS Code, Nano and SSH are external programs; install them
separately when wanted. Windows curl is discovered when available. K Shell is
not a Linux/WSL environment and does not run Linux ELF programs. Pipes, shell
redirection, variable expansion and full Zsh compatibility are not implemented.
Use curl -o file URL for downloads. Quote URLs containing & and filenames with
spaces. Fonts/background are settings of your terminal host.

HISTORY / REMOVAL
History uses %APPDATA%\mysh\history.txt for compatibility. It is not part of this
ZIP. To remove the application, close K Shell and remove its installation folder
and its K Shell Terminal fragment folder. Keep your history if you want it.

SHARING / VERIFICATION
Share the original ZIP and its matching .sha256 file. In the ZIP's download
folder, display its hash with:
   Get-FileHash .\KShell-__VERSION__-windows-x64.zip -Algorithm SHA256
Compare it with the accompanying checksum. This build is not code-signed.
LICENSE contains the application's MIT license; licenses/ contains dependency
license texts and THIRD-PARTY-NOTICES.txt lists their declared licenses.
