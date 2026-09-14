$ErrorActionPreference = "Stop"

cargo build --release --locked

$outputDirectory = Join-Path $PSScriptRoot "dist"
New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
$executable = Join-Path $PSScriptRoot "target\release\foto-acte.exe"
$destination = Join-Path $outputDirectory "Foto-acte-3x4.exe"
Copy-Item -LiteralPath $executable -Destination $destination -Force

Write-Host "Executabil creat: dist\Foto-acte-3x4.exe"
