[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Archive)

$ErrorActionPreference = 'Stop'
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('kshell-package-test-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($testRoot) | Out-Null
function Assert-True($Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
try {
    $extractRoot = Join-Path $testRoot 'extracted package with spaces'
    Expand-Archive -LiteralPath $Archive -DestinationPath $extractRoot
    $package = @(Get-ChildItem -LiteralPath $extractRoot -Directory)
    Assert-True ($package.Count -eq 1) 'Expected one portable package directory.'
    $packageRoot = $package[0].FullName
    $installer = Join-Path $packageRoot 'Install.ps1'
    $install = Join-Path $testRoot 'installed with spaces'
    $profile = Join-Path $testRoot 'terminal\K Shell\kshell.json'
    & $installer -InstallDirectory $install -TerminalProfilePath $profile -FontFace 'Consolas'
    $executable = Join-Path $install 'kshell.exe'
    Assert-True (Test-Path -LiteralPath $executable) 'Installed executable is missing.'
    $manifest = Get-Content -LiteralPath (Join-Path $packageRoot 'package-manifest.json') -Raw | ConvertFrom-Json
    Assert-True ((Get-FileHash -LiteralPath $executable).Hash -eq $manifest.sha256) 'Installed binary differs from package.'
    $settings = Get-Content -LiteralPath $profile -Raw | ConvertFrom-Json
    Assert-True ($settings.profiles[0].commandline.Contains($executable)) 'Profile points at the download instead of the installed copy.'
    Assert-True ($settings.profiles[0].commandline.Contains('--icons unicode')) 'Standard font must use standard icons.'
    Assert-True ($settings.profiles[0].font.face -eq 'Consolas') 'Selected font was not applied.'
    Assert-True (Test-Path -LiteralPath (Join-Path $install 'licenses\rustyline-19.0.0\LICENSE')) 'Installed dependency license text is missing.'
    Assert-True (Test-Path -LiteralPath (Join-Path $install 'licenses\rust-standard-library\COPYRIGHT-library.html')) 'Rust standard-library notices are missing.'
    $oldTestPath = $env:PATH
    try {
        $env:PATH = ''
        $help = & $executable --help
        Assert-True ($LASTEXITCODE -eq 0 -and ($help -join "`n").Contains('K Shell')) 'Portable binary failed without developer tools on PATH.'
    } finally { $env:PATH = $oldTestPath }
    Push-Location $install
    try {
        & $executable -c 'write package-test.txt portable-ready'
        Assert-True ($LASTEXITCODE -eq 0) 'Installed shell could not write a fixture file.'
        Assert-True ([IO.File]::ReadAllText((Join-Path $install 'package-test.txt')) -eq "portable-ready`n") 'Installed shell wrote unexpected contents.'
    } finally { Pop-Location }
    # Upgrades preserve both unrelated files and a previous executable.
    & $installer -InstallDirectory $install -TerminalProfilePath $profile -FontFace 'Consolas'
    Assert-True (@(Get-ChildItem -LiteralPath $install -Filter 'kshell-previous-*.exe').Count -eq 1) 'Upgrade did not preserve the old executable.'
    Assert-True (Test-Path -LiteralPath (Join-Path $install 'package-test.txt')) 'Upgrade removed an unrelated file.'
    Assert-True (@(Get-ChildItem -LiteralPath (Split-Path -Parent $profile) -Filter '*.bak').Count -eq 1) 'Profile upgrade backup is missing.'
    $plainInstall = Join-Path $testRoot 'without terminal profile'
    $unusedProfile = Join-Path $testRoot 'must-not-create-profile.json'
    & $installer -InstallDirectory $plainInstall -TerminalProfilePath $unusedProfile -NoTerminalProfile
    Assert-True (Test-Path -LiteralPath (Join-Path $plainInstall 'kshell.exe')) 'Application-only installation failed.'
    Assert-True (-not (Test-Path -LiteralPath $unusedProfile)) 'NoTerminalProfile unexpectedly created a profile.'
    $protected = Join-Path $testRoot 'existing user folder'
    [IO.Directory]::CreateDirectory($protected) | Out-Null
    [IO.File]::WriteAllText((Join-Path $protected 'README.txt'), 'keep user content')
    $rejected = $false
    try { & $installer -InstallDirectory $protected -NoTerminalProfile } catch { $rejected = $true }
    Assert-True $rejected 'Unmarked nonempty folder was not protected.'
    Assert-True ([IO.File]::ReadAllText((Join-Path $protected 'README.txt')) -eq 'keep user content') 'Existing file was overwritten.'
    $badDestination = Join-Path $testRoot 'must not be installed'
    $manifestPath = Join-Path $packageRoot 'package-manifest.json'
    $manifest.sha256 = '0' * 64
    [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json))
    $rejected = $false
    try { & $installer -InstallDirectory $badDestination -NoTerminalProfile } catch { $rejected = $true }
    Assert-True $rejected 'Corrupted package manifest was accepted.'
    Assert-True (-not (Test-Path -LiteralPath $badDestination)) 'Invalid package wrote installation files.'
    Write-Output 'Distribution checks passed: extraction, private install, executable, profile/font fallback, license texts, upgrade/backups, user-file protection and integrity rejection.'
} finally {
    # Remove only the fresh, exclusively created fixture inside the system temp
    # directory. Never expand deletion from a caller-supplied destination.
    $tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    $resolvedFixture = [IO.Path]::GetFullPath($testRoot)
    if (-not $resolvedFixture.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($resolvedFixture) -notmatch '^kshell-package-test-[0-9a-f]{32}$') { throw 'Refusing unsafe fixture cleanup.' }
    Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
}
