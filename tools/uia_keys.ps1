<#
Test clavier d'une application : met sa fenêtre au premier plan, envoie une suite de touches (syntaxe SendKeys)
et relève après chacune l'élément qui a le focus (UI Automation). À n'utiliser que sur ThunderLook, jamais sur Outlook.
Exemple : powershell -File tools/uia_keys.ps1 -Process thunderlook -Keys '{F6}','{F6}','^2'
#>
param(
    [string]$Process = 'thunderlook',
    [Parameter(Mandatory)][string[]]$Keys,
    [int]$DelayMs = 400
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Fg {
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
}
'@
$proc = Get-Process $Process | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { throw "Pas de fenêtre pour $Process" }
$A = [System.Windows.Automation.AutomationElement]
$win = $A::FromHandle($proc.MainWindowHandle)
# La fenêtre WebView2 refuse SetFocus : on vise le premier champ de saisie, ce qui l'amène au premier plan.
$edit = $win.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.PropertyCondition($A::ControlTypeProperty, [System.Windows.Automation.ControlType]::Edit)))
[void][Fg]::SetForegroundWindow($proc.MainWindowHandle)
if ($edit) { $edit.SetFocus() } else { $win.SetFocus() }
Start-Sleep -Milliseconds 600

# SendKeys frappe dans la fenêtre au premier plan, quelle qu'elle soit : on vérifie avant chaque touche
# qu'il s'agit bien de l'application visée (une fenêtre d'une autre application peut surgir pendant le test).
function Assert-Foreground {
    $fpid = 0
    [void][Fg]::GetWindowThreadProcessId([Fg]::GetForegroundWindow(), [ref]$fpid)
    if ($fpid -ne $proc.Id) {
        $other = (Get-Process -Id $fpid -ErrorAction SilentlyContinue).ProcessName
        throw "Arrêt : la fenêtre au premier plan appartient à « $other » (pid $fpid), pas à $Process. Aucune touche envoyée."
    }
}

function Describe-Focus {
    $f = $A::FocusedElement
    if (-not $f) { return '(aucun)' }
    $c = $f.Current
    $sel = ''
    $p = $null
    if ($f.TryGetCurrentPattern([System.Windows.Automation.SelectionPattern]::Pattern, [ref]$p)) {
        $s = $p.Current.GetSelection() | ForEach-Object { $_.Current.Name } | Select-Object -First 1
        if ($s) { $sel = " ; sélection=« $s »" }
    }
    "{0} « {1} »{2}" -f $c.ControlType.ProgrammaticName.Replace('ControlType.', ''), $c.Name, $sel
}

"départ : " + (Describe-Focus)
foreach ($k in $Keys) {
    Assert-Foreground
    [System.Windows.Forms.SendKeys]::SendWait($k)
    Start-Sleep -Milliseconds $DelayMs
    "{0,-8} → {1}" -f $k, (Describe-Focus)
}
