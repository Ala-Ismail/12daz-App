<# Verify the approved executable before inserting it into the original build. #>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$OriginalArchive,
    [Parameter(Mandatory)][string]$SignedExecutable,
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$ExpectedPublisher,
    [Parameter(Mandatory)][string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$') { throw 'Invalid package version' }
if ([string]::IsNullOrWhiteSpace($ExpectedPublisher)) { throw 'Expected publisher is required' }
$archive = (Resolve-Path -LiteralPath $OriginalArchive).Path
$signedExe = (Resolve-Path -LiteralPath $SignedExecutable).Path
$info = [Diagnostics.FileVersionInfo]::GetVersionInfo($signedExe)
if ($info.ProductName -cne '12dAZ Converter' -or $info.ProductVersion -cne $Version -or
    $info.FileVersion -cne $Version -or $info.CompanyName -cne 'Lambda Technologies Pty Ltd' -or
    $info.OriginalFilename -cne '12dAZ-Converter.exe') {
    throw 'Signed executable product/version metadata does not match this release'
}
$signature = Get-AuthenticodeSignature -LiteralPath $signedExe
if ($signature.Status -ne 'Valid' -or -not $signature.SignerCertificate -or -not $signature.TimeStamperCertificate) {
    throw 'A valid trusted Authenticode signature and timestamp are required'
}
$publisher = $signature.SignerCertificate.GetNameInfo([Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false)
if ($publisher -cne $ExpectedPublisher) { throw 'Unexpected signing certificate publisher' }

# Use a fresh private working directory. Never remove or overwrite caller directories.
$work = Join-Path ([IO.Path]::GetTempPath()) ('12daz-signed-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $work
$payload = Join-Path $work 'payload'
Expand-Archive -LiteralPath $archive -DestinationPath $payload
$folder = Join-Path $payload "12dAZ-Converter-$Version"
$executable = Join-Path $folder '12dAZ-Converter.exe'
$pythonLayout = Test-Path -LiteralPath (Join-Path $folder '_internal') -PathType Container
$rustCli = Join-Path $folder 'az12-converter.exe'
$rustLayout = (Test-Path -LiteralPath $rustCli -PathType Leaf) -and
    (Test-Path -LiteralPath (Join-Path $folder 'THIRD_PARTY_NOTICES.txt') -PathType Leaf)
if (-not (Test-Path -LiteralPath $executable -PathType Leaf) -or ($pythonLayout -eq $rustLayout)) {
    throw 'Original package layout is invalid'
}
if ($rustLayout) {
    $cliInfo = [Diagnostics.FileVersionInfo]::GetVersionInfo($rustCli)
    if ($cliInfo.ProductVersion -cne $Version -or $cliInfo.OriginalFilename -cne 'az12-converter.exe') {
        throw 'Standalone CLI metadata does not match this release'
    }
}
Copy-Item -LiteralPath $signedExe -Destination $executable -Force
$report = Join-Path $work 'signed-smoke.json'
$smoke = Start-Process -FilePath $executable -ArgumentList @('--smoke-test', "`"$report`"") -WindowStyle Hidden -PassThru
$processHandle = $smoke.Handle
if (-not $smoke.WaitForExit(60000)) {
    Stop-Process -Id $smoke.Id -ErrorAction SilentlyContinue
    throw 'Signed application smoke test timed out'
}
if ($smoke.ExitCode -ne 0) { throw "Signed application smoke test failed: $($smoke.ExitCode)" }
$result = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
if (-not $result.passed -or $result.application_version -ne $Version) {
    throw 'Signed application smoke result failed or reported the wrong version'
}

if (Test-Path -LiteralPath $OutputDirectory) { throw 'Output directory must not already exist' }
$null = New-Item -ItemType Directory -Path $OutputDirectory
$packageName = "12dAZ-Converter-$Version-Windows-x64"
$finalArchive = Join-Path $OutputDirectory "$packageName.zip"
Compress-Archive -LiteralPath $folder -DestinationPath $finalArchive
$roundtrip = Join-Path $work 'roundtrip'
Expand-Archive -LiteralPath $finalArchive -DestinationPath $roundtrip
$packagedExe = Join-Path $roundtrip "12dAZ-Converter-$Version/12dAZ-Converter.exe"
if ((Get-FileHash -LiteralPath $packagedExe -Algorithm SHA256).Hash -ne
    (Get-FileHash -LiteralPath $signedExe -Algorithm SHA256).Hash) {
    throw 'Final ZIP did not preserve the signed executable'
}
# Check all bundled files, not just the signed EXE, survived final packaging.
$expectedFiles = @(Get-ChildItem -LiteralPath $payload -File -Recurse -Force)
$actualFiles = @(Get-ChildItem -LiteralPath $roundtrip -File -Recurse -Force)
if ($expectedFiles.Count -ne $actualFiles.Count) { throw 'Final ZIP file count changed' }
foreach ($file in $expectedFiles) {
    $relative = $file.FullName.Substring($payload.Length + 1)
    $copy = Join-Path $roundtrip $relative
    if (-not (Test-Path -LiteralPath $copy -PathType Leaf) -or
        (Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash -ne
        (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash) {
        throw "Final ZIP altered or omitted bundled file: $relative"
    }
}
$hash = (Get-FileHash -LiteralPath $finalArchive -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText((Join-Path (Resolve-Path -LiteralPath $OutputDirectory).Path 'SHA256SUMS.txt'), "$hash  $packageName.zip`n", [Text.Encoding]::ASCII)
Write-Output "Verified signed package created for publisher $publisher. Temporary evidence: $work"
# Hosted runners discard this temporary evidence when the job ends.
