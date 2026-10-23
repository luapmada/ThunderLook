# Lance les lots cloud demandes (par defaut K a O), un par un, et ouvre l'onglet de suivi de chaque session.
# Usage : .\lancer-cloud.ps1 [K L M N O]
param([string[]]$Lots = @('K','L','M','N','O'))
Set-Location $PSScriptRoot
$urls = @()
foreach ($l in $Lots) {
    Write-Host "== Lot $l" -ForegroundColor Cyan
    $log = Join-Path $env:TEMP "cloud-lot-$l.txt"
    Start-Transcript -Path $log -Force | Out-Null
    claude --cloud "Sur main, lis notes/cloud/lot-$l.md et execute-le."
    Stop-Transcript | Out-Null
    $m = Select-String -Path $log -Pattern 'View:\s*(https://\S+)' | Select-Object -Last 1
    if ($m) {
        $url = $m.Matches[0].Groups[1].Value
        $urls += $url
        Start-Process $url
    } else {
        Write-Host "Lot $l : adresse de suivi introuvable." -ForegroundColor Yellow
    }
}
if ($urls.Count -lt $Lots.Count) { Start-Process "https://claude.ai/code" }
Write-Host "Sessions lancees : $($urls.Count)/$($Lots.Count). Quand elles ont fini, dis a Claude : lots $($Lots -join ' ') termines." -ForegroundColor Green
