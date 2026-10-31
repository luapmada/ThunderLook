# Bilan apres une synchro MPE du S23, en lecture seule : ce qui manque au S23 par rapport a Outlook.
# Lien par EntryID : MPE garde celui de chaque element dans ses fichiers (OUTLOOK-UID, X-OUTLOOK-UID).
# Usage : powershell -ExecutionPolicy Bypass -File tools\s23_check.ps1

$known = @('m. marquis', 'adi mme lajoie')  # versions de paul@ deja reportees dans le PST (31/10)
$dir = Join-Path $env:APPDATA 'MyPhoneExplorer\samsung SM-S916U1 [6C568FCE-C43F-452D-8649-A69C40379DF6]'
$utf8 = [Text.Encoding]::UTF8
$cards = [IO.File]::ReadAllText("$dir\Phonebook.dat", $utf8)
$org = [IO.File]::ReadAllText("$dir\Organizer.dat", $utf8)
$before = 90; $after = 90
foreach ($l in [IO.File]::ReadAllLines("$dir\settings.dat")) {
    if ($l -match '^SyncDaysBefore=(\d+)') { $before = [int]$Matches[1] }
    if ($l -match '^SyncDaysAfter=(\d+)') { $after = [int]$Matches[1] }
}
$onPhone = @{}
foreach ($m in [regex]::Matches($cards + $org, 'OUTLOOK-UID:([0-9A-Fa-f]+)')) { $onPhone[$m.Groups[1].Value.ToUpper()] = 1 }

$ol = New-Object -ComObject Outlook.Application
$ns = $ol.GetNamespace('MAPI')
$mine = $null; $pst = $null
foreach ($st in $ns.Stores) {
    if ($st.DisplayName -like 'paul*') { $mine = $st.GetRootFolder() }
    if ($st.DisplayName -like 'Fichier*') { $pst = $st.GetRootFolder() }
}

function Rows($folder, $cols) {
    $t = $folder.GetTable()
    $t.Columns.RemoveAll() | Out-Null
    foreach ($c in $cols) { $t.Columns.Add($c) | Out-Null }
    while (-not $t.EndOfTable) {
        $x = $t.GetNextRow(); $r = @{}
        foreach ($c in $cols) { $r[$c] = $x.Item($c) }
        $r
    }
}
function Key($s) { ([string]$s -replace '\s', '').ToLower() }
function Calendars($root) {
    foreach ($f in $root.Folders) {
        if ($f.DefaultItemType -eq 1) { $f; foreach ($u in $f.Folders) { if ($u.DefaultItemType -eq 1) { $u } } }
    }
}

$gaps = 0
function Report($title, $lines) {
    $lines = @($lines | Where-Object { $_ })
    "$title : $($lines.Count)"
    $lines | Sort-Object | ForEach-Object { "  $_" }
    $script:gaps += $lines.Count
}

$pstContacts = $pst.Folders.Item('Contacts')
$cols = 'EntryID', 'FullName', 'MessageClass'
$inPst = @{}
$rows = @(Rows $pstContacts $cols)
foreach ($r in $rows) { $inPst[(Key $r.FullName)] = 1 }
Report 'Contacts seulement dans le dossier par defaut (paul@), absents du PST donc du S23' (
    Rows ($mine.Folders | Where-Object { $_.DefaultItemType -eq 2 } | Select-Object -First 1) $cols |
        Where-Object { -not $inPst.ContainsKey((Key $_.FullName)) -and (([string]$_.FullName -replace '\s+', ' ').Trim().ToLower()) -notin $known } |
        ForEach-Object { $_.FullName })
Report 'Contacts du PST absents du S23 (hors listes de distribution)' (
    $rows | Where-Object { $_.MessageClass -notlike 'IPM.DistList*' -and -not $onPhone.ContainsKey(([string]$_.EntryID).ToUpper()) } | ForEach-Object { $_.FullName })

$cols = 'EntryID', 'Subject', 'Start', 'IsRecurring'
$events = @(Calendars $pst | ForEach-Object { $name = $_.Name; Rows $_ $cols | ForEach-Object { $_.Folder = $name; $_ } })
$inPst = @{}
foreach ($e in $events) { $inPst[(Key $e.Subject) + '|' + ([datetime]$e.Start).ToString('yyyyMMddHHmm')] = 1 }
Report 'Rendez-vous seulement dans le calendrier par defaut (paul@)' (
    Rows ($mine.Folders | Where-Object { $_.DefaultItemType -eq 1 } | Select-Object -First 1) $cols |
        Where-Object { -not $inPst.ContainsKey((Key $_.Subject) + '|' + ([datetime]$_.Start).ToString('yyyyMMddHHmm')) } |
        ForEach-Object { '{0:yyyy-MM-dd HH:mm} {1}' -f ([datetime]$_.Start), $_.Subject })

$from = (Get-Date).AddDays(-$before); $to = (Get-Date).AddDays($after)
$missing = $events | Where-Object { -not $onPhone.ContainsKey(([string]$_.EntryID).ToUpper()) }
$inside = @($missing | Where-Object { $_.IsRecurring -or ([datetime]$_.Start -ge $from -and [datetime]$_.Start -le $to) })
Report "Rendez-vous du PST absents du S23 dans la fenetre (-$before / +$after jours) ou en serie" (
    $inside | ForEach-Object { '{0} : {1:yyyy-MM-dd HH:mm} {2}' -f $_.Folder, ([datetime]$_.Start), $_.Subject })
$outside = @($missing).Count - $inside.Count
"Rendez-vous du PST hors fenetre, non synchronises par reglage MPE : $outside"
''
if ($gaps -eq 0) { 'OK : aucun ecart.' } else { "A TRAITER : $gaps ecart(s)." }
