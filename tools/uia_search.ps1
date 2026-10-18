<#
Saisit une recherche dans la zone « Rechercher » de ThunderLook par UI Automation (ValuePattern), sans clavier :
aucune touche n'est envoyée, donc rien ne peut atterrir dans une autre application. La recherche part après le
délai de frappe de l'interface.
Exemple : powershell -File tools/uia_search.ps1 -Text 'facture pj:oui' ; pour effacer : -Clear
#>
param(
    [string]$Process = 'thunderlook',
    [string]$Text = '',
    [switch]$Clear,
    [int]$WaitMs = 1500
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$proc = Get-Process $Process | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { throw "Pas de fenêtre pour $Process" }
$A = [System.Windows.Automation.AutomationElement]
$win = $A::FromHandle($proc.MainWindowHandle)
$cond = New-Object System.Windows.Automation.AndCondition(
    (New-Object System.Windows.Automation.PropertyCondition($A::ControlTypeProperty, [System.Windows.Automation.ControlType]::Edit)),
    (New-Object System.Windows.Automation.PropertyCondition($A::NameProperty, 'Rechercher')))
$edit = $win.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond)
if (-not $edit) { throw 'Zone de recherche introuvable' }
$vp = $edit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
if (-not $Clear -and -not $Text) { throw 'Préciser -Text ou -Clear' }
$vp.SetValue($(if ($Clear) { '' } else { $Text }))
Start-Sleep -Milliseconds $WaitMs
"recherche « $Text » saisie"
