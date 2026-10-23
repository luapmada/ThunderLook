# Lance les lots cloud F à J, un par un (une session cloud par lot).
Set-Location $PSScriptRoot
foreach ($l in "F","G","H","I","J") {
    Write-Host "== Lot $l" -ForegroundColor Cyan
    claude --cloud "Sur main, lis notes/cloud/lot-$l.md et exécute-le."
}
Write-Host "Les 5 sessions cloud sont lancées. Dis à Claude : lots F à J terminés (quand ils le sont)." -ForegroundColor Green
