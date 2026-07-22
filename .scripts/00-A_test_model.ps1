Set-Location "D:\weaver-sync\development\personal\reposmerge-rs"
cargo test -p reposmerge model 2>&1
Write-Output "=== EXIT: $LASTEXITCODE ==="
