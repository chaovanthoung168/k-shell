#Requires -Version 5.1
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Executable)

$ErrorActionPreference = 'Stop'
$Executable = (Resolve-Path -LiteralPath $Executable).ProviderPath
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('kshell-bundled-test-' + [Guid]::NewGuid().ToString('N'))
$previousKpmHome = $env:KSHELL_KPM_HOME
$previousConfig = $env:KSHELL_CONFIG_DIR
try {
    # Read-only commands must work in the shipped binary without downloads,
    # separate executables, or an existing user configuration/tool store.
    $env:KSHELL_KPM_HOME = Join-Path $testRoot 'kpm'
    $env:KSHELL_CONFIG_DIR = Join-Path $testRoot 'config'
    foreach ($command in @('kreq', 'kpm')) {
        $help = & $Executable -c "$command --help" 2>&1
        if ($LASTEXITCODE -ne 0 -or ($help -join "`n") -notmatch "(?m)^\s+$command ") {
            throw "The packaged executable is missing the $command builtin. Build the current source before publishing."
        }
    }
    $catalog = & $Executable -c 'kpm list' 2>&1
    if ($LASTEXITCODE -ne 0) { throw 'The bundled KPM catalog did not run.' }
    foreach ($tool in @('node', 'python', 'go', 'rust', 'php', 'git', 'java')) {
        if (($catalog -join "`n") -notmatch "(?m)^$tool\s") { throw "The bundled KPM catalog is missing $tool." }
    }
    if (Test-Path -LiteralPath $testRoot) { throw 'Bundled command checks unexpectedly created configuration or tool storage.' }
    Write-Output 'Bundled command checks passed: kreq help, kpm help and all seven KPM catalog entries, without installing tools.'
} finally {
    $env:KSHELL_KPM_HOME = $previousKpmHome
    $env:KSHELL_CONFIG_DIR = $previousConfig
}
