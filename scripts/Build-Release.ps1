[CmdletBinding()]
param([string]$OutputDirectory)

$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repository 'target\packages' }
$outputRoot = [IO.Path]::GetFullPath($OutputDirectory)
$cargo = (Get-Command cargo.exe -ErrorAction SilentlyContinue).Source
if (-not $cargo) { $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
$manifestText = [IO.File]::ReadAllText((Join-Path $repository 'Cargo.toml'))
$version = [regex]::Match($manifestText, '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"').Groups[1].Value
if (-not $version) { throw 'Cannot read package version.' }
$packageName = "KShell-$version-windows-x64"
$oldTarget = $env:CARGO_TARGET_DIR
$oldFlags = $env:CARGO_ENCODED_RUSTFLAGS
$oldIncremental = $env:CARGO_INCREMENTAL
Push-Location $repository
try {
    $env:CARGO_TARGET_DIR = Join-Path $repository 'target\distribution'
    $env:CARGO_INCREMENTAL = '0'
    $buildFlags = @('-C','target-feature=+crt-static','-C','strip=symbols',"--remap-path-prefix=$repository=/kshell")
    $cargoDirectory = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
    $buildFlags += "--remap-path-prefix=$cargoDirectory=/cargo"
    $env:CARGO_ENCODED_RUSTFLAGS = $buildFlags -join [char]31
    & $cargo build --release --locked --offline --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $metadataText = & $cargo metadata --locked --offline --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Dependency metadata could not be read.' }
    $metadata = $metadataText | ConvertFrom-Json
    $binary = Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-msvc\release\kshell.exe'
    [IO.Directory]::CreateDirectory($outputRoot) | Out-Null
    $staging = Join-Path $outputRoot ('staging-' + [Guid]::NewGuid().ToString('N'))
    $package = Join-Path $staging $packageName
    [IO.Directory]::CreateDirectory($package) | Out-Null
    Copy-Item -LiteralPath $binary -Destination (Join-Path $package 'kshell.exe')
    foreach ($file in @('Install.ps1','README.txt')) {
        Copy-Item -LiteralPath (Join-Path $repository ('contrib\distribution\' + $file)) -Destination (Join-Path $package $file)
    }
    $guidePath = Join-Path $package 'README.txt'
    [IO.File]::WriteAllText($guidePath, ([IO.File]::ReadAllText($guidePath).Replace('__VERSION__', $version)), [Text.UTF8Encoding]::new($false))
    Copy-Item -LiteralPath (Join-Path $repository 'contrib\windows-terminal\Install-KShellProfile.ps1') -Destination (Join-Path $package 'Install-KShellProfile.ps1')
    Copy-Item -LiteralPath (Join-Path $repository 'LICENSE') -Destination (Join-Path $package 'LICENSE')
    $notices = @('K Shell third-party notices', 'License texts are in licenses/. This list includes the locked dependency graph (including platform-specific dependencies).', '')
    foreach ($dependency in @($metadata.packages | Where-Object { $_.name -ne 'kshell' } | Sort-Object name)) {
        $noticeName = $dependency.name + '-' + $dependency.version
        if ($noticeName -notmatch '^[A-Za-z0-9_.+\-]+$') { throw 'Unexpected dependency name.' }
        $licenseDirectory = Join-Path $package ('licenses\' + $noticeName)
        [IO.Directory]::CreateDirectory($licenseDirectory) | Out-Null
        $dependencyRoot = Split-Path -Parent $dependency.manifest_path
        $licenses = @(Get-ChildItem -LiteralPath $dependencyRoot -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE)' })
        if ($dependency.license_file) {
            $explicitLicense = Join-Path $dependencyRoot $dependency.license_file
            if (Test-Path -LiteralPath $explicitLicense -PathType Leaf) { $licenses += Get-Item -LiteralPath $explicitLicense }
        }
        if ($licenses.Count -eq 0) {
            $overrideDirectory = Join-Path $repository ('contrib\distribution\license-overrides\' + $noticeName)
            if (Test-Path -LiteralPath $overrideDirectory -PathType Container) {
                $licenses = @(Get-ChildItem -LiteralPath $overrideDirectory -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE)' })
            }
        }
        if ($licenses.Count -eq 0) { throw "Missing license texts for $noticeName" }
        foreach ($license in @($licenses | Sort-Object FullName -Unique)) {
            Copy-Item -LiteralPath $license.FullName -Destination (Join-Path $licenseDirectory $license.Name)
        }
        $notices += "$($dependency.name) $($dependency.version): $($dependency.license); $($dependency.repository)"
    }
    $rustc = Join-Path (Split-Path -Parent $cargo) 'rustc.exe'
    $sysroot = & $rustc --print sysroot
    if ($LASTEXITCODE -ne 0) { throw 'Rust standard-library license location is unavailable.' }
    $standardLicenses = Join-Path $package 'licenses\rust-standard-library'
    [IO.Directory]::CreateDirectory($standardLicenses) | Out-Null
    Copy-Item -LiteralPath (Join-Path $sysroot 'share\doc\rust\COPYRIGHT-library.html') -Destination (Join-Path $standardLicenses 'COPYRIGHT-library.html')
    foreach ($standardLicense in @('MIT.txt','Apache-2.0.txt','LLVM-exception.txt')) {
        Copy-Item -LiteralPath (Join-Path $sysroot ('share\doc\rust\licenses\' + $standardLicense)) -Destination (Join-Path $standardLicenses $standardLicense)
    }
    $notices += 'Rust standard library: see licenses/rust-standard-library/COPYRIGHT-library.html for its components and license notices.'
    [IO.File]::WriteAllLines((Join-Path $package 'THIRD-PARTY-NOTICES.txt'), $notices, [Text.UTF8Encoding]::new($false))
    $packageManifest = [ordered]@{ application = 'K Shell'; version = $version; platform = 'windows-x64'; sha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash }
    [IO.File]::WriteAllText((Join-Path $package 'package-manifest.json'), ($packageManifest | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
    $zip = Join-Path $outputRoot ($packageName + '.zip')
    if (Test-Path -LiteralPath $zip) { throw "Package already exists; choose a fresh -OutputDirectory: $zip" }
    Compress-Archive -LiteralPath $package -DestinationPath $zip -CompressionLevel Optimal
    $hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText(($zip + '.sha256'), ($hash + '  ' + [IO.Path]::GetFileName($zip) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
    Write-Output "Share: $zip"
    Write-Output "SHA256: $hash"
} finally {
    $env:CARGO_TARGET_DIR = $oldTarget
    $env:CARGO_ENCODED_RUSTFLAGS = $oldFlags
    $env:CARGO_INCREMENTAL = $oldIncremental
    Pop-Location
}
