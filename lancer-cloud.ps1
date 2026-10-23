# Lance les lots cloud F à J, un par un, et ouvre l'onglet de suivi de chaque session.
Set-Location $PSScriptRoot
$urls = @()
foreach ($l in "F","G","H","I","J") {
    Write-Host "== Lot $l" -ForegroundColor Cyan
    $log = Join-Path $env:TEMP "cloud-lot-$l.txt"
    Start-Transcript -Path $log -Force | Out-Null
    claude --cloud "Sur main, lis notes/cloud/lot-$l.md et exécute-le."
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
if ($urls.Count -lt 5) { Start-Process "https://claude.ai/code" }
Write-Host "Sessions lancées : $($urls.Count)/5. Quand elles ont fini, dis à Claude : lots F à J terminés." -ForegroundColor Green
