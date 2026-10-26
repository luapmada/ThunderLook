<#
Declenche la multi-synchronisation native de MyPhoneExplorer (MPE), qui n'a pas de minuterie.
Principe : PostMessage(WM_COMMAND, 44) sur la fenetre principale (classe ThunderRT6FormDC, titre "MyPhoneExplorer - ...").
44 = identifiant du menu Outils > Commencer la multi-synchronisation (Maj+F5), constate sur le PC de Paul.
Aucune souris, aucun clavier systeme (pas de SendKeys, pas de SendInput). settings.dat et general.ini en lecture seule.

Ne fait RIEN (et le journalise) si :
  - MyPhoneExplorer.exe ne tourne pas ;
  - aucune fenetre ThunderRT6FormDC visible, ou aucune dont le titre commence par "MyPhoneExplorer - " ;
  - une autre fenetre visible de MPE est ouverte, ou la fenetre principale est desactivee (dialogue modal :
    synchro en cours ou confirmation en attente) ;
  - LastContactSync (settings.dat de l'appareil LastUser) date de moins de -MinMinutes minutes, est illisible
    ou est dans le futur.

Exemples :
  powershell -NoProfile -ExecutionPolicy Bypass -File tools\mpe_trigger.ps1 -DryRun
  powershell -NoProfile -ExecutionPolicy Bypass -File tools\mpe_trigger.ps1

Les messages sont volontairement en ASCII (PowerShell 5.1 lit un .ps1 sans BOM en ANSI) ; un caractere accentue
eventuel passerait par [char] (ex. [char]233 pour e accent aigu).
Code retour : 0 = rien a faire ou commande envoyee (ou simulee), 1 = erreur.
#>
param(
    [switch]$DryRun,
    [int]$MinMinutes = 50,
    [string]$DataDir = (Join-Path $env:APPDATA 'MyPhoneExplorer'),
    [string]$LogPath = (Join-Path $env:LOCALAPPDATA 'ThunderLook\mpe_trigger.log')
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Text;
using System.Runtime.InteropServices;
public static class MpeWin {
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern bool IsWindowEnabled(IntPtr h);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", EntryPoint = "PostMessageW")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);

    public class Info {
        public long Handle; public uint Pid; public string Class; public string Title; public bool Enabled;
    }
    // Fenetres de premier niveau visibles appartenant aux processus donnes.
    public static List<Info> VisibleTopLevel(uint[] pids) {
        List<Info> res = new List<Info>();
        EnumWindows(delegate(IntPtr h, IntPtr l) {
            uint pid; GetWindowThreadProcessId(h, out pid);
            if (Array.IndexOf(pids, pid) < 0 || !IsWindowVisible(h)) return true;
            StringBuilder c = new StringBuilder(256); GetClassNameW(h, c, 256);
            StringBuilder t = new StringBuilder(512); GetWindowTextW(h, t, 512);
            Info i = new Info();
            i.Handle = h.ToInt64(); i.Pid = pid; i.Class = c.ToString(); i.Title = t.ToString(); i.Enabled = IsWindowEnabled(h);
            res.Add(i);
            return true;
        }, IntPtr.Zero);
        return res;
    }
}
'@

function Write-Log([string]$Msg) {
    $line = '{0} {1}{2}' -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $(if ($DryRun) { '[DRYRUN] ' } else { '' }), $Msg
    try {
        $dir = Split-Path -Parent $LogPath
        if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
        if ((Test-Path -LiteralPath $LogPath) -and ((Get-Item -LiteralPath $LogPath).Length -gt 524288)) {
            Move-Item -LiteralPath $LogPath -Destination ($LogPath + '.old') -Force
        }
        Add-Content -LiteralPath $LogPath -Value $line -Encoding UTF8
    } catch { }
    Write-Host $line
}

# Une seule instance a la fois.
$mutex = New-Object System.Threading.Mutex($false, 'Local\ThunderLook_mpe_trigger')
if (-not $mutex.WaitOne(0)) { Write-Log 'Rien : une autre instance du declencheur tourne.'; exit 0 }

try {
    # 1. Processus
    $procs = @(Get-Process -Name 'MyPhoneExplorer' -ErrorAction SilentlyContinue)
    if ($procs.Count -eq 0) { Write-Log 'Rien : MyPhoneExplorer.exe ne tourne pas.'; exit 0 }
    [uint32[]]$pids = $procs | ForEach-Object { [uint32]$_.Id }

    # 2. Fenetres
    $wins = @([MpeWin]::VisibleTopLevel($pids))
    $vb = @($wins | Where-Object { $_.Class -eq 'ThunderRT6FormDC' })
    if ($vb.Count -eq 0) { Write-Log 'Rien : aucune fenetre ThunderRT6FormDC visible (MPE reduit dans la zone de notification ?).'; exit 0 }
    $main = @($vb | Where-Object { $_.Title.StartsWith('MyPhoneExplorer - ') } | Select-Object -First 1)
    if ($main.Count -eq 0) {
        Write-Log ('Rien : aucune fenetre ThunderRT6FormDC visible dont le titre commence par "MyPhoneExplorer - " (titres : ' + (($vb | ForEach-Object { '"' + $_.Title + '"' }) -join ', ') + ').')
        exit 0
    }
    $main = $main[0]
    $others = @($wins | Where-Object { $_.Handle -ne $main.Handle })
    if ($others.Count -gt 0) {
        Write-Log ('Rien : fenetre(s) de dialogue MPE ouverte(s) (synchro en cours ou confirmation en attente) : ' +
            (($others | ForEach-Object { '[' + $_.Class + '] "' + $_.Title + '"' }) -join ', '))
        exit 0
    }
    if (-not $main.Enabled) { Write-Log 'Rien : la fenetre principale de MPE est desactivee (dialogue modal en cours).'; exit 0 }

    # 3. Derniere synchro (lecture seule)
    $ini = Join-Path $DataDir 'general.ini'
    if (-not (Test-Path -LiteralPath $ini)) { Write-Log ('Rien : general.ini introuvable (' + $ini + ').'); exit 0 }
    $latin1 = [System.Text.Encoding]::GetEncoding(28591)
    $m = [regex]::Match([System.IO.File]::ReadAllText($ini, $latin1), '(?m)^\s*LastUser=(.+?)\s*$')
    if (-not $m.Success) { Write-Log 'Rien : cle LastUser absente de general.ini.'; exit 0 }
    $device = $m.Groups[1].Value
    $settings = Join-Path (Join-Path $DataDir $device) 'settings.dat'
    if (-not (Test-Path -LiteralPath $settings)) { Write-Log ('Rien : settings.dat introuvable (' + $settings + ').'); exit 0 }
    $fs = New-Object System.IO.FileStream($settings, 'Open', 'Read', 'ReadWrite')
    try {
        $sr = New-Object System.IO.StreamReader($fs, $latin1)
        $text = $sr.ReadToEnd()
    } finally { $fs.Dispose() }
    $m = [regex]::Match($text, '(?m)^\s*LastContactSync=(\d{14})')
    if (-not $m.Success) { Write-Log ('Rien : LastContactSync absent ou illisible dans le settings.dat de "' + $device + '".'); exit 0 }
    $last = [datetime]::MinValue
    if (-not [datetime]::TryParseExact($m.Groups[1].Value, 'ddMMyyyyHHmmss', [Globalization.CultureInfo]::InvariantCulture, 'None', [ref]$last)) {
        Write-Log ('Rien : LastContactSync non interpretable (' + $m.Groups[1].Value + ').'); exit 0
    }
    $age = (Get-Date) - $last
    if ($age.TotalMinutes -lt 0) { Write-Log ('Rien : LastContactSync dans le futur (' + $last.ToString('s') + ').'); exit 0 }
    if ($age.TotalMinutes -lt $MinMinutes) {
        Write-Log ('Rien : derniere synchro il y a {0:N0} min (< {1} min), appareil "{2}".' -f $age.TotalMinutes, $MinMinutes, $device)
        exit 0
    }

    # 4. Envoi de la commande
    $info = 'fenetre 0x{0:X} "{1}", derniere synchro il y a {2:N0} min, appareil "{3}"' -f $main.Handle, $main.Title, $age.TotalMinutes, $device
    if ($DryRun) { Write-Log ('Enverrait WM_COMMAND 44 : ' + $info); exit 0 }
    $ok = [MpeWin]::PostMessage([IntPtr]$main.Handle, 0x111, [IntPtr]44, [IntPtr]::Zero)
    if ($ok) { Write-Log ('WM_COMMAND 44 envoye : ' + $info) } else { Write-Log ('ERREUR : PostMessage a echoue (code ' + [Runtime.InteropServices.Marshal]::GetLastWin32Error() + ').'); exit 1 }
    exit 0
} catch {
    Write-Log ('ERREUR : ' + $_.Exception.Message)
    exit 1
} finally {
    $mutex.ReleaseMutex(); $mutex.Dispose()
}
