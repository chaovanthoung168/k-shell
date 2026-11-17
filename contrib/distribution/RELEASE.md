K Shell 0.1.7 includes the native `kreq` HTTP client and K Shell Package Manager
(`kpm`) in the downloadable Windows executable. Both are builtins: no separate
installation is needed. The v0.1.6 binary did not contain these commands even
after their source was added to the main branch.

After upgrading, exit any running K Shell session and launch K Shell again. Try:

```text
kreq --help
kpm list
kpm install
```

KPM manages Node.js, Python, Go, Rust, PHP, Git, and Java (Temurin JDK 17+), with
provider-specific limits explained by `kpm --help`. `kreq` supports HTTP requests,
headers, JSON formatting, response views, and downloads without wrapping curl.

Packaging now checks that both commands and the KPM catalog work in the release
binary and in private installations before publication. The bootstrap installer
also prints the installed version so older downloads can be identified.

Install K Shell on Windows 10/11 x64 from PowerShell:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/chaovanthoung168/k-shell/main/install.ps1 | iex"
```

Restart your terminal app, then run `kshell`, or select the **K Shell** Windows Terminal profile.

The installer verifies the ZIP's SHA256 checksum, installs for your account, adds the installation folder to your user PATH, and registers a Windows Terminal profile. Administrator access, Rust, WSL, and new fonts are not required. The execution-policy option applies only to the installer process.

For a portable session, download the ZIP and matching `.sha256`, compare with `Get-FileHash`, extract the entire archive, and run `kshell.exe`. To install manually, run the included `Install.ps1 -PathScope User` from PowerShell.

This is an unsigned early Windows x64 release. K Shell supports single commands, native Windows applications, history, completion, themes, and Linux-style utilities. Pipes, shell redirection, variable expansion, and full scripting are still under development. External developer tools are installed separately.
