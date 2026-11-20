[CmdletBinding()]
param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '../dist/rust-candidate'),
    [switch]$Offline
)

$ErrorActionPreference = 'Stop'
$project = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$manifest = Join-Path $project 'rust/Cargo.toml'
$target = 'x86_64-pc-windows-msvc'
$locked = @('--manifest-path', $manifest, '--locked', '--features', 'desktop')
if ($Offline) { $locked += '--offline' }

$metadata = cargo metadata @locked --format-version 1 --filter-platform $target | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
$byId = @{}
foreach ($package in $metadata.packages) { $byId.Add($package.id, $package) }
$root = $byId[$metadata.resolve.root]
$version = $root.version
if ($version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$') { throw 'Expected a numeric Windows release version' }

$null = New-Item -ItemType Directory -Force -Path $OutputDirectory
$destination = (Resolve-Path -LiteralPath $OutputDirectory).Path
$name = "12dAZ-Converter-$version-Windows-x64"
$zip = Join-Path $destination "$name.zip"
$sums = Join-Path $destination 'SHA256SUMS.txt'
if ((Test-Path -LiteralPath $zip) -or (Test-Path -LiteralPath $sums)) {
    throw 'Choose an output directory without existing candidate artifacts'
}

cargo build @locked --release --target $target
if ($LASTEXITCODE -ne 0) { throw 'Rust Windows build failed' }
$build = Join-Path $metadata.target_directory "$target/release"
$stage = Join-Path $destination ('.stage-' + [Guid]::NewGuid().ToString('N'))
$folder = Join-Path $stage "12dAZ-Converter-$version"
$null = New-Item -ItemType Directory -Path $folder

try {
    foreach ($binary in @('12dAZ-Converter.exe', 'az12-converter.exe')) {
        $source = Join-Path $build $binary
        $info = [Diagnostics.FileVersionInfo]::GetVersionInfo($source)
        if ($info.FileVersion -cne $version -or $info.ProductVersion -cne $version -or
            $info.ProductName -cne '12dAZ Converter' -or $info.CompanyName -cne 'Lambda Technologies Pty Ltd' -or
            $info.OriginalFilename -cne $binary) {
            throw "Unexpected Windows release metadata in $binary"
        }
        Copy-Item -LiteralPath $source -Destination $folder
    }
    Copy-Item -LiteralPath (Join-Path $project 'LICENSE') -Destination (Join-Path $folder 'LICENSE.txt')
    Copy-Item -LiteralPath (Join-Path $project 'rust/resources/user_guide.txt') -Destination $folder

    $graph = @{}
    foreach ($node in $metadata.resolve.nodes) { $graph.Add($node.id, $node) }
    $pending = [Collections.Generic.Stack[string]]::new()
    $seen = [Collections.Generic.HashSet[string]]::new()
    $pending.Push($root.id)
    while ($pending.Count -gt 0) {
        $id = $pending.Pop()
        if (-not $seen.Add($id)) { continue }
        foreach ($dependency in $graph[$id].deps) {
            if (@($dependency.dep_kinds | Where-Object { $null -eq $_.kind }).Count -gt 0) { $pending.Push($dependency.pkg) }
        }
    }

    $notice = [Text.StringBuilder]::new()
    $null = $notice.AppendLine('Third-party notices for the 12dAZ Converter Rust Windows x64 distribution.').AppendLine()
    $null = $notice.AppendLine('=== CADCraft map code ===').AppendLine(
        [IO.File]::ReadAllText((Join-Path $project 'rust/licenses/CADCraft-MIT.txt'))
    ).AppendLine()
    $null = $notice.AppendLine('=== Google Maps attribution artwork ===').AppendLine(
        'Official Google Maps attribution artwork is displayed with Google Maps Platform tiles. See https://developers.google.com/maps/documentation/tile/policies.'
    ).AppendLine()
    foreach ($package in @($seen | ForEach-Object { $byId[$_] } | Sort-Object name, version)) {
        if ($package.id -eq $root.id) { continue }
        $null = $notice.AppendLine("=== $($package.name) $($package.version) ===")
        $null = $notice.AppendLine("License: $($package.license)")
        if ($package.source -like 'registry+*') {
            $null = $notice.AppendLine("Corresponding source: https://crates.io/api/v1/crates/$($package.name)/$($package.version)/download")
        }
        $directory = Split-Path -Parent $package.manifest_path
        $files = @(Get-ChildItem -LiteralPath $directory -Recurse -Depth 3 -File |
            Where-Object { $_.Name -match '^(LICEN[SC]E|COPYING|COPYRIGHT|NOTICE|OFL|UNLICENSE)([._ -]|$)' } |
            Sort-Object FullName)
        if ($package.license_file) {
            $explicit = Join-Path $directory $package.license_file
            if (Test-Path -LiteralPath $explicit -PathType Leaf) { $files += Get-Item -LiteralPath $explicit }
        }
        foreach ($file in @($files | Sort-Object FullName -Unique)) {
            $relative = $file.FullName.Substring($directory.Length + 1)
            $null = $notice.AppendLine("--- $relative ---").AppendLine([IO.File]::ReadAllText($file.FullName))
        }
        $null = $notice.AppendLine()
    }
    [IO.File]::WriteAllText((Join-Path $folder 'THIRD_PARTY_NOTICES.txt'), $notice.ToString())

    $stagedZip = Join-Path $stage "$name.zip"
    Compress-Archive -LiteralPath $folder -DestinationPath $stagedZip -CompressionLevel Optimal
    $roundtrip = Join-Path $stage 'roundtrip'
    Expand-Archive -LiteralPath $stagedZip -DestinationPath $roundtrip
    $extracted = Join-Path $roundtrip "12dAZ-Converter-$version"
    $expected = @(Get-ChildItem -LiteralPath $folder -File)
    if (@(Get-ChildItem -LiteralPath $extracted -File).Count -ne $expected.Count) { throw 'Portable ZIP file count differs' }
    foreach ($file in $expected) {
        $copy = Join-Path $extracted $file.Name
        if ((Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash) {
            throw "Portable ZIP changed $($file.Name)"
        }
    }
    $hash = (Get-FileHash -LiteralPath $stagedZip -Algorithm SHA256).Hash.ToLowerInvariant()
    $stagedSums = Join-Path $stage 'SHA256SUMS.txt'
    [IO.File]::WriteAllText($stagedSums, "$hash  $name.zip`n", [Text.Encoding]::ASCII)
    Move-Item -LiteralPath $stagedZip -Destination $zip
    Move-Item -LiteralPath $stagedSums -Destination $sums
    Write-Output "Built $zip"
} finally {
    $actualStage = (Resolve-Path -LiteralPath $stage).Path
    if (-not $actualStage.StartsWith($destination.TrimEnd('\') + '\.stage-', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Unexpected staging path'
    }
    Remove-Item -LiteralPath $actualStage -Recurse -Force
}
