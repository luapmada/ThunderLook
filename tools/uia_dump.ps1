<#
Relevé de l'arbre UI Automation et capture de la fenêtre principale d'une application en cours d'exécution.
Lecture seule, sauf -Action : sélection d'un onglet (tab:<nom>) ou invocation d'un bouton (invoke:<nom>).
Les sorties brutes contiennent des données personnelles : notes/uia-raw/ et captures/ sont ignorés par git.

Exemple : powershell -File tools/uia_dump.ps1 -Process OUTLOOK -Label accueil
#>
param(
    [string]$Process = 'OUTLOOK',
    [Parameter(Mandatory)][string]$Label,
    [string]$Action = '',
    [int]$MaxNodes = 8000,
    [int]$MaxDepth = 30,
    [string]$OutDir = (Split-Path $PSScriptRoot -Parent)
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Win32Capture {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr ctx);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
'@
# Per-monitor DPI v2 (-4) : coordonnées et capture en pixels physiques
[Win32Capture]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null

$proc = Get-Process $Process | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { throw "Pas de fenêtre principale pour $Process" }
$hwnd = $proc.MainWindowHandle
$A = [System.Windows.Automation.AutomationElement]
$top = $A::FromHandle($hwnd)

function Find-ByName([string]$name, $type) {
    $c1 = New-Object System.Windows.Automation.PropertyCondition($A::NameProperty, $name)
    $c2 = New-Object System.Windows.Automation.PropertyCondition($A::ControlTypeProperty, $type)
    $and = New-Object System.Windows.Automation.AndCondition($c1, $c2)
    $top.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $and)
}

if ($Action -like 'tab:*') {
    $el = Find-ByName $Action.Substring(4) ([System.Windows.Automation.ControlType]::TabItem)
    if (-not $el) { throw "Onglet introuvable : $Action" }
    $el.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    Start-Sleep -Milliseconds 800
} elseif ($Action -like 'invoke:*') {
    $el = Find-ByName $Action.Substring(7) ([System.Windows.Automation.ControlType]::Button)
    if (-not $el) { throw "Bouton introuvable : $Action" }
    $el.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 1500
}

# Arbre (vue « contrôle »), parcours en profondeur borné
$walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
$rows = New-Object System.Collections.Generic.List[string]
$rows.Add("depth`tcontrolType`tname`tautomationId`tclassName`taccessKey`tacceleratorKey`tenabled`tx`ty`tw`th")
$count = 0
function Visit($el, [int]$depth) {
    if ($script:count -ge $MaxNodes -or $depth -gt $MaxDepth) { return }
    $script:count++
    try {
        $c = $el.Current
        $r = $c.BoundingRectangle
        $vals = @($depth, $c.ControlType.ProgrammaticName.Replace('ControlType.', ''), $c.Name, $c.AutomationId, $c.ClassName,
                  $c.AccessKey, $c.AcceleratorKey, $c.IsEnabled,
                  [int]$r.X, [int]$r.Y, [int]$r.Width, [int]$r.Height) | ForEach-Object { "$_" -replace "[`t`r`n]", ' ' }
        $rows.Add($vals -join "`t")
    } catch { return }
    $child = $walker.GetFirstChild($el)
    while ($child -ne $null) {
        Visit $child ($depth + 1)
        $child = $walker.GetNextSibling($child)
    }
}
Visit $top 0

$uiaDir = Join-Path $OutDir 'notes\uia-raw'
$capDir = Join-Path $OutDir 'captures'
New-Item -ItemType Directory -Force $uiaDir, $capDir | Out-Null
$tsv = Join-Path $uiaDir "$Label.tsv"
[IO.File]::WriteAllLines($tsv, $rows, (New-Object System.Text.UTF8Encoding($false)))

# Capture de la fenêtre (PrintWindow, PW_RENDERFULLCONTENT = 2)
$rect = New-Object Win32Capture+RECT
[Win32Capture]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
$w = $rect.R - $rect.L; $h = $rect.B - $rect.T
$bmp = New-Object System.Drawing.Bitmap($w, $h)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
[Win32Capture]::PrintWindow($hwnd, $hdc, 2) | Out-Null
$g.ReleaseHdc($hdc); $g.Dispose()
$png = Join-Path $capDir "$Label.png"
$bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()

"noeuds=$count fenetre=${w}x${h} tsv=$tsv png=$png"
