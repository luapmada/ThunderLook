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
$proc = Get-Process $Process | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { throw "Pas de fenêtre pour $Process" }
$A = [System.Windows.Automation.AutomationElement]
$win = $A::FromHandle($proc.MainWindowHandle)
# La fenêtre WebView2 refuse SetFocus : on vise le premier champ de saisie, ce qui l'amène au premier plan.
$edit = $win.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.PropertyCondition($A::ControlTypeProperty, [System.Windows.Automation.ControlType]::Edit)))
if ($edit) { $edit.SetFocus() } else { $win.SetFocus() }
Start-Sleep -Milliseconds 600

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
    [System.Windows.Forms.SendKeys]::SendWait($k)
    Start-Sleep -Milliseconds $DelayMs
    "{0,-8} → {1}" -f $k, (Describe-Focus)
}
