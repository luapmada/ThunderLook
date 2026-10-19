<#
Active un bouton ou un onglet de ThunderLook par son nom (UI Automation : Invoke, Toggle ou SelectionItem), sans
clavier, puis relève les lignes de la liste des messages rendues à l'écran (nom accessible et sélection).
Exemples :
  powershell -File tools/uia_invoke.ps1 -Name 'Affichage','Afficher en tant que conversations'
  powershell -File tools/uia_invoke.ps1 -List 20
#>
param(
    [string]$Process = 'thunderlook',
    [string[]]$Name = @(),
    [int]$List = 0,
    [int]$WaitMs = 800,
    # Vrai clic de souris sur le n-ième élément (à partir de 0) dont le nom contient -Click, au point
    # (gauche + OffsetX, haut + OffsetY), ou au centre sans décalage. Refusé si la fenêtre n'est pas au premier plan.
    [string]$Click = '',
    [int]$Index = 0,
    [int]$OffsetX = -1,
    [int]$OffsetY = -1
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Mouse {
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, UIntPtr e);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
'@
# UI Automation donne des pixels physiques ; sans cela, SetCursorPos les prendrait pour des pixels logiques et
# le clic partirait ailleurs dès que l'affichage est mis à l'échelle (125 % ici, 2026-10-19).
[void][Mouse]::SetProcessDPIAware()
$proc = Get-Process $Process | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { throw "Pas de fenêtre pour $Process" }
$A = [System.Windows.Automation.AutomationElement]
$P = [System.Windows.Automation.PropertyCondition]
$win = $A::FromHandle($proc.MainWindowHandle)
$scope = [System.Windows.Automation.TreeScope]::Descendants

foreach ($n in $Name) {
    $el = $win.FindFirst($scope, (New-Object $P($A::NameProperty, $n)))
    if (-not $el) { throw "Élément « $n » introuvable" }
    $done = $false
    foreach ($pat in @([System.Windows.Automation.InvokePattern]::Pattern,
                       [System.Windows.Automation.TogglePattern]::Pattern,
                       [System.Windows.Automation.SelectionItemPattern]::Pattern)) {
        $obj = $null
        if ($el.TryGetCurrentPattern($pat, [ref]$obj)) {
            switch ($pat.ProgrammaticName) {
                'InvokePatternIdentifiers.Pattern' { $obj.Invoke() }
                'TogglePatternIdentifiers.Pattern' { $obj.Toggle() }
                default { $obj.Select() }
            }
            "« $n » : $($pat.ProgrammaticName -replace 'PatternIdentifiers.Pattern','')"
            $done = $true
            break
        }
    }
    if (-not $done) { throw "« $n » n'offre ni Invoke, ni Toggle, ni SelectionItem" }
    Start-Sleep -Milliseconds $WaitMs
}

if ($Click) {
    # Fenêtre réduite : ses éléments sont hors écran ; on la restaure avant de mesurer.
    if ([Mouse]::IsIconic($proc.MainWindowHandle)) {
        [void][Mouse]::ShowWindow($proc.MainWindowHandle, 9)
        Start-Sleep -Milliseconds 800
    }
    $all = $win.FindAll($scope, [System.Windows.Automation.Condition]::TrueCondition)
    $hits = @($all | Where-Object { $_.Current.Name -like "*$Click*" })
    if ($hits.Count -le $Index) { throw "« $Click » : $($hits.Count) élément(s), index $Index absent" }
    $r = $hits[$Index].Current.BoundingRectangle
    $x = [int]$(if ($OffsetX -ge 0) { $r.Left + $OffsetX } else { $r.Left + $r.Width / 2 })
    $y = [int]$(if ($OffsetY -ge 0) { $r.Top + $OffsetY } else { $r.Top + $r.Height / 2 })
    $wr = $win.Current.BoundingRectangle
    if (-not ($x -gt $wr.Left -and $x -lt $wr.Right -and $y -gt $wr.Top -and $y -lt $wr.Bottom)) {
        throw "Arrêt : point $x,$y hors de la fenêtre de $Process ($wr). Aucun clic."
    }
    [void][Mouse]::SetForegroundWindow($proc.MainWindowHandle)
    # Windows refuse souvent SetForegroundWindow ; le focus UIA sur un champ amène la fenêtre au premier plan.
    $edit = $win.FindFirst($scope, (New-Object $P($A::ControlTypeProperty, [System.Windows.Automation.ControlType]::Edit)))
    if ($edit) { $edit.SetFocus() }
    Start-Sleep -Milliseconds 400
    $fpid = 0
    [void][Mouse]::GetWindowThreadProcessId([Mouse]::GetForegroundWindow(), [ref]$fpid)
    if ($fpid -ne $proc.Id) { throw "Arrêt : la fenêtre au premier plan n'est pas $Process (pid $fpid). Aucun clic." }
    [void][Mouse]::SetCursorPos($x, $y)
    [Mouse]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero)
    [Mouse]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero)
    "clic sur « $($hits[$Index].Current.Name) » ($($hits[$Index].Current.ControlType.ProgrammaticName)) en $x,$y"
    Start-Sleep -Milliseconds $WaitMs
}

if ($List -gt 0) {
    $box = $win.FindFirst($scope, (New-Object $P($A::ControlTypeProperty, [System.Windows.Automation.ControlType]::List)))
    if (-not $box) { throw 'Liste des messages introuvable' }
    $items = $box.FindAll([System.Windows.Automation.TreeScope]::Children,
        (New-Object $P($A::ControlTypeProperty, [System.Windows.Automation.ControlType]::ListItem)))
    $i = 0
    foreach ($it in $items) {
        if ($i -ge $List) { break }
        $sel = $null
        $mark = ''
        if ($it.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$sel) -and $sel.Current.IsSelected) { $mark = '> ' }
        "$mark$($it.Current.Name)"
        $i++
    }
}
