<#
Ouvre un menu (MenuItem / SplitButton) par son nom via UI Automation, liste ses entrées,
et peut en activer une (-Choose). Sert à basculer temporairement l'affichage du ruban.
Exemple : powershell -File tools/uia_menu.ps1 -Menu "Options d'affichage du ruban" [-Choose "Ruban classique"]
#>
param(
    [string]$Process = 'OUTLOOK',
    [Parameter(Mandatory)][string]$Menu,
    [string]$Choose = ''
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$A = [System.Windows.Automation.AutomationElement]
$proc = Get-Process $Process | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
$top = $A::FromHandle($proc.MainWindowHandle)
$cond = New-Object System.Windows.Automation.PropertyCondition($A::NameProperty, $Menu)
$el = $top.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond)
if (-not $el) { throw "Menu introuvable : $Menu" }
$p = $null
if ($el.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$p)) { $p.Expand() }
elseif ($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) { $p.Invoke() }
Start-Sleep -Milliseconds 800

# Les menus déroulants Office sont des fenêtres de premier niveau du même processus
$pidCond = New-Object System.Windows.Automation.PropertyCondition($A::ProcessIdProperty, $proc.Id)
$items = @()
foreach ($w in $A::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $pidCond)) {
    $mi = New-Object System.Windows.Automation.PropertyCondition($A::ControlTypeProperty, [System.Windows.Automation.ControlType]::MenuItem)
    $items += $w.FindAll([System.Windows.Automation.TreeScope]::Descendants, $mi)
}
$items | ForEach-Object { "- " + $_.Current.Name + $(if ($_.Current.IsEnabled) { '' } else { ' (désactivé)' }) }

if ($Choose) {
    $target = $items | Where-Object { $_.Current.Name -eq $Choose } | Select-Object -First 1
    if (-not $target) { throw "Entrée introuvable : $Choose" }
    $q = $null
    if ($target.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$q)) { $q.Invoke() }
    elseif ($target.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$q)) { $q.Select() }
    elseif ($target.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$q)) { $q.Toggle() }
    Start-Sleep -Milliseconds 1200
    "choisi : $Choose"
} else {
    if ($el.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$p)) { $p.Collapse() }
}
