# Build the Microsoft Store package (.msix) from a release build of the app.
#
#   pwsh app/windows/msix/pack.ps1 -Exe target/release/kjv-interlinear.exe
#
# The package is unsigned: Partner Center signs Store packages itself.
param(
    [Parameter(Mandatory)] [string] $Exe,
    [string] $Out = ""
)
$ErrorActionPreference = "Stop"
$here = $PSScriptRoot
$app = Resolve-Path (Join-Path $here "..\..")

# Store versions are four numbers and the last must be 0: 0.2.0 -> 0.2.0.0
$version = (Get-Content (Join-Path $app "tauri.conf.json") -Raw | ConvertFrom-Json).version
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw "unexpected version '$version' in tauri.conf.json" }
$version = "$version.0"
if (-not $Out) { $Out = "KJV-Interlinear_$($version)_x64.msix" }

$layout = Join-Path ([IO.Path]::GetTempPath()) "kjv-interlinear-msix"
Remove-Item $layout -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory (Join-Path $layout "Assets") | Out-Null
Copy-Item $Exe (Join-Path $layout "kjv-interlinear.exe")
foreach ($logo in "Square44x44Logo", "Square150x150Logo", "StoreLogo") {
    Copy-Item (Join-Path $app "icons\$logo.png") (Join-Path $layout "Assets\$logo.png")
}
$manifest = (Get-Content (Join-Path $here "AppxManifest.xml") -Raw).Replace('$VERSION$', $version)
[IO.File]::WriteAllText((Join-Path $layout "AppxManifest.xml"), $manifest, (New-Object Text.UTF8Encoding $false))

# makeappx from the newest installed Windows SDK
$makeappx = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" |
    Sort-Object { [version]$_.Directory.Parent.Name } | Select-Object -Last 1
if (-not $makeappx) { throw "makeappx.exe not found: install the Windows SDK" }
& $makeappx.FullName pack /d $layout /p $Out /o /h SHA256
if ($LASTEXITCODE -ne 0) { throw "makeappx failed ($LASTEXITCODE)" }
Write-Output "Built $Out (version $version)"
