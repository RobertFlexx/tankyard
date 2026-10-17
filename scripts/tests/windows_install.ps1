# Exercise the real installer against the just-built archive before publishing.
param([Parameter(Mandatory)][string]$Version, [string]$PackageDir = "dist")
$ErrorActionPreference = "Stop"
$Work = Join-Path ([IO.Path]::GetTempPath()) ("tankyard installer " + [guid]::NewGuid())
$FixtureDir = Join-Path $Work "release"
$Destination = Join-Path $Work "installed game"
$Installer = Join-Path $PSScriptRoot "../../install.ps1"
$FixtureTag = $Version
function Invoke-RestMethod { return @{ tag_name = $FixtureTag } }
function Invoke-WebRequest {
    param([string]$Uri, [string]$OutFile, [switch]$UseBasicParsing)
    $File = ([Uri]$Uri).Segments[-1]
    Copy-Item (Join-Path $FixtureDir $File) $OutFile -Force
}
try {
    New-Item -ItemType Directory -Force $FixtureDir | Out-Null
    $Archive = "tankyard-$Version-windows-x86_64.zip"
    Copy-Item (Join-Path $PackageDir $Archive) $FixtureDir
    $Checksum = Get-Content (Join-Path $PackageDir "$Archive.sha256")
    Set-Content (Join-Path $FixtureDir "SHA256SUMS") $Checksum
    & $Installer -Version latest -InstallDir $Destination -NoShortcut -NoPath
    $Settings = Join-Path $Destination "settings.json"
    Set-Content $Settings '{"player_name":"Installer test"}'
    & $Installer -Version $Version -InstallDir $Destination -NoShortcut -NoPath
    & $Installer -Version $Version -InstallDir $Destination -NoShortcut -NoPath -Server
    foreach ($Binary in @("tankyard", "tankyard-server", "tankyard-relay")) {
        $Reported = & "$Destination/$Binary.exe" --version
        if ($LASTEXITCODE -ne 0 -or $Reported -notmatch [regex]::Escape($Version.TrimStart('v'))) {
            throw "Installed $Binary did not start with the expected version."
        }
    }
    if ((Get-Content $Settings) -ne '{"player_name":"Installer test"}') { throw "Settings changed." }
    if (-not (Test-Path "$Destination/tankyard.ico" -PathType Leaf)) { throw "Installed icon missing." }
    $Before = (Get-FileHash "$Destination/tankyard.exe").Hash
    Set-Content (Join-Path $FixtureDir "SHA256SUMS") (("0" * 64) + "  " + $Archive)
    $Rejected = $false
    try { & $Installer -Version $Version -InstallDir $Destination -NoShortcut -NoPath }
    catch { $Rejected = $_.Exception.Message -match 'Checksum mismatch' }
    if (-not $Rejected) { throw "Corrupt download was accepted." }
    if ((Get-FileHash "$Destination/tankyard.exe").Hash -ne $Before) { throw "Failed update changed the game." }
    Write-Host "Windows install, reinstall, server install, icon and checksum checks passed."
}
finally { if (Test-Path $Work) { Remove-Item -Recurse -Force $Work } }
