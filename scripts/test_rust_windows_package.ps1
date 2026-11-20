[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Archive,
    [string]$Evidence,
    [string]$ExpectedPublisher
)

$ErrorActionPreference = 'Stop'
$archivePath = (Resolve-Path -LiteralPath $Archive).Path
$name = [IO.Path]::GetFileName($archivePath)
if ($name -notmatch '^12dAZ-Converter-([0-9]+\.[0-9]+\.[0-9]+)-Windows-x64\.zip$') { throw 'Invalid archive name' }
$version = $Matches[1]
$sums = Join-Path (Split-Path -Parent $archivePath) 'SHA256SUMS.txt'
$hash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
if ((Get-Content -LiteralPath $sums -Raw).Trim() -cne "$hash  $name") { throw 'Package checksum differs' }

$scratch = [IO.Path]::GetTempPath()
$specialName = '12daz-package-' + [Guid]::NewGuid().ToString('N') + ' ' + [char]0x6e2c + [char]0x91cf
$work = Join-Path $scratch $specialName
$null = New-Item -ItemType Directory -Path $work
try {
    Expand-Archive -LiteralPath $archivePath -DestinationPath $work
    $folder = Join-Path $work "12dAZ-Converter-$version"
    $names = @(Get-ChildItem -LiteralPath $folder -File | Select-Object -ExpandProperty Name)
    $expected = @('12dAZ-Converter.exe', 'az12-converter.exe', 'LICENSE.txt', 'THIRD_PARTY_NOTICES.txt', 'user_guide.txt')
    if ((Compare-Object $names $expected) -or @(Get-ChildItem -LiteralPath $folder -Directory).Count -ne 0) {
        throw 'Unexpected files in portable package'
    }
    foreach ($binary in @('12dAZ-Converter.exe', 'az12-converter.exe')) {
        $info = [Diagnostics.FileVersionInfo]::GetVersionInfo((Join-Path $folder $binary))
        if ($info.ProductVersion -cne $version -or $info.FileVersion -cne $version -or
            $info.CompanyName -cne 'Lambda Technologies Pty Ltd' -or $info.OriginalFilename -cne $binary) {
            throw "Invalid executable metadata in $binary"
        }
    }
    $notices = Get-Content -LiteralPath (Join-Path $folder 'THIRD_PARTY_NOTICES.txt') -Raw
    if ($notices -notmatch 'acadrust' -or $notices -notmatch 'Mozilla Public License') { throw 'Required runtime notices are missing' }

    $gui = Join-Path $folder '12dAZ-Converter.exe'
    $cli = Join-Path $folder 'az12-converter.exe'
    $signature = Get-AuthenticodeSignature -LiteralPath $gui
    if ($ExpectedPublisher) {
        if ($signature.Status -ne 'Valid' -or -not $signature.TimeStamperCertificate -or
            $signature.SignerCertificate.GetNameInfo('SimpleName', $false) -cne $ExpectedPublisher) {
            throw 'Invalid publisher signature or timestamp'
        }
    }

    $initialPath = $env:Path
    try {
        $env:Path = Join-Path $env:SystemRoot 'System32'
        $cliVersion = & $cli --version
        if ($LASTEXITCODE -ne 0 -or ($cliVersion -join '') -cne "12daz-converter $version") { throw 'Packaged CLI failed its version check' }
        $report = Join-Path $work 'desktop-smoke.json'
        $process = Start-Process -FilePath $gui -ArgumentList @('--smoke-test', ('"{0}"' -f $report)) -WindowStyle Hidden -PassThru
        $null = $process.Handle
        if (-not $process.WaitForExit(60000)) {
            Stop-Process -InputObject $process -Force
            throw 'Packaged desktop smoke test timed out'
        }
        if ($process.ExitCode -ne 0) {
            $details = if (Test-Path -LiteralPath $report) { Get-Content -LiteralPath $report -Raw } else { 'Smoke report missing' }
            throw "Packaged desktop failed with $($process.ExitCode): $details"
        }
        $smoke = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
        if (-not $smoke.passed -or $smoke.application_version -cne $version) { throw 'Packaged smoke evidence did not match the release' }
    } finally { $env:Path = $initialPath }

    $result = [ordered]@{
        passed = $true
        version = $version
        package_sha256 = $hash
        gui_signature = $signature.Status.ToString()
        host_os_version = [Environment]::OSVersion.VersionString
        powershell_process_elevated = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
        non_system_path_entries_removed = $true
        smoke = $smoke
    } | ConvertTo-Json -Depth 5
    if ($Evidence) { [IO.File]::WriteAllText([IO.Path]::GetFullPath($Evidence), $result) }
    Write-Output $result
} finally {
    $actualWork = (Resolve-Path -LiteralPath $work).Path
    $expectedWork = Join-Path (Resolve-Path -LiteralPath $scratch).Path $specialName
    if ($actualWork -ine $expectedWork) { throw 'Unexpected package verification path' }
    Remove-Item -LiteralPath $actualWork -Recurse -Force
}
