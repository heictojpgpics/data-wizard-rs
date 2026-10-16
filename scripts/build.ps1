# Data Wizard release build (DiskGenie flow): the full gate battery,
# the per-user NSIS installer, the Store-ready MSIX, the portable zip
# and SHA256SUMS256.txt. Windows only (MSVC + WebView2).
#
#   scripts/build.ps1                 # x64 (default)
#   scripts/build.ps1 -Arch x64
#
# Requires: Rust (stable-msvc), Node 22+, npm. Optional signtool for
# authenticode signing (un-signed builds are fine for private testing
# — SmartScreen guidance lives in the README).

param(
    [ValidateSet("x64")]
    [string]$Arch = "x64",
    [string]$AppRoot = (Split-Path $PSScriptRoot -Parent),
    [switch]$SkipGates   # never use for releases; dev speed only
)

$ErrorActionPreference = "Stop"
$target = "x86_64-pc-windows-msvc"
$toml = Join-Path $AppRoot "src-tauri/Cargo.toml"
Set-Location $AppRoot

if (-not $SkipGates) {
    Write-Host "== Gates: cargo fmt / clippy / tests =="
    Push-Location (Join-Path $AppRoot "src-tauri")
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt --check failed" }
    cargo clippy --target $target -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "clippy failed" }
    Pop-Location
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw "workspace fmt failed" }
    cargo clippy --workspace --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "workspace clippy failed" }
    cargo test --workspace
    if ($LASTEXITCODE -ne 0) { throw "workspace tests failed" }

    Write-Host "== Gates: typecheck / lint / build (ui) =="
    npm --prefix ui run typecheck
    if ($LASTEXITCODE -ne 0) { throw "ui typecheck failed" }
    npm --prefix ui run lint
    if ($LASTEXITCODE -ne 0) { throw "ui lint failed" }
    npm --prefix ui run build
    if ($LASTEXITCODE -ne 0) { throw "ui build failed" }
}

Write-Host "== Tauri build ($target, NSIS per-user) =="
npx tauri build --target $target
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

# ---- Version + MSIX layout ------------------------------------------------
$version = (Select-String -Path $toml -Pattern '^version\s*=\s*"([^"]+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value
$msixVersion = "$version.0"
$bundle = Join-Path $AppRoot "src-tauri/target/$target/release/bundle"
$outDir = Join-Path $AppRoot "dist-release"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

# ---- NSIS installer -------------------------------------------------------
$installer = Get-ChildItem (Join-Path $bundle "nsis") -Filter "*.exe" |
    Where-Object { $_.Name -notlike "*uninstall*" } | Select-Object -First 1
if (-not $installer) { throw "NSIS installer not found" }
Copy-Item $installer.FullName -Destination (Join-Path $outDir "DataWizard-$version-$Arch-setup.exe") -Force

# ---- Store-ready MSIX (MakeAppx over the committed manifest) --------------
Write-Host "== MSIX (MakeAppx) =="
$layout = Join-Path $AppRoot "msix-layout"
Remove-Item $layout -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path "$layout\Assets" | Out-Null
Copy-Item (Join-Path $AppRoot "src-tauri/target/$target/release/dw-tauri.exe") -Destination (Join-Path $layout "dw-tauri.exe") -Force

Add-Type -AssemblyName System.Drawing
$iconSrc = [System.Drawing.Bitmap]::FromFile((Join-Path $AppRoot "packaging\icon\icon-source.png"))
function Save-Png([int]$size, [string]$name) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.DrawImage($iconSrc, 0, 0, $size, $size)
    $bmp.Save((Join-Path $layout "Assets\$name"), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
}
Save-Png 44  "Square44x44Logo.targetsize-44.png"
Save-Png 44  "Square44x44Logo.targetsize-44_altform-unplated.png"
Save-Png 48  "Square44x44Logo.targetsize-48.png"
Save-Png 88  "Square44x44Logo.targetsize-88.png"
Save-Png 176 "Square44x44Logo.targetsize-176.png"
Save-Png 150 "Square150x150Logo.png"
Save-Png 310 "Square310x310Logo.png"
Save-Png 50  "StoreLogo.png"
$iconSrc.Dispose()

$manifest = (Get-Content -Raw (Join-Path $AppRoot "packaging\msix\AppxManifest.xml")) `
    -replace '__IDENTITY_NAME__', 'DataWizard' `
    -replace '__PUBLISHER__', 'CN=Data Wizard CI' `
    -replace '__VERSION__', $msixVersion
$manifest | Set-Content -Encoding UTF8 (Join-Path $layout "AppxManifest.xml")

$makeappx = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\MakeAppx.exe" |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $makeappx) { throw "MakeAppx.exe not found (Windows SDK)" }
& $makeappx.FullName pack /o /v /d $layout /p (Join-Path $outDir "DataWizard-$msixVersion-$Arch.msix")
if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed" }
Remove-Item $layout -Recurse -Force

# ---- Portable zip ----------------------------------------------------------
Write-Host "== Portable zip =="
$exe = Join-Path $AppRoot "src-tauri/target/$target/release/dw-tauri.exe"
if (-not (Test-Path $exe)) { throw "release exe not found" }
$portable = Join-Path $outDir "portable"
New-Item -ItemType Directory -Force -Path $portable | Out-Null
Copy-Item $exe -Destination (Join-Path $portable "DataWizard.exe") -Force
@(
    "Data Wizard portable. Runs without installing; WebView2 is required",
    "(the Windows bootstrapper is bundled with the setup exe instead)."
) | Set-Content (Join-Path $portable "README.txt")
Compress-Archive -Path $portable -DestinationPath (Join-Path $outDir "DataWizard-$version-$Arch-portable.zip") -Force
Remove-Item $portable -Recurse -Force

# ---- Checksums -------------------------------------------------------------
Write-Host "== SHA256SUMS256.txt =="
$sums = Get-ChildItem $outDir -File | ForEach-Object {
    $h = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
    "$h  $($_.Name)"
}
$sums | Set-Content (Join-Path $outDir "SHA256SUMS256.txt")
$sums | Write-Host

Write-Host "Release artifacts in $outDir"
