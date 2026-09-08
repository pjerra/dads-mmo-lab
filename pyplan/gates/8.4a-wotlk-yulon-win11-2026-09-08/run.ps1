param([string]$Driver, [string]$Stage, [string]$Extra = "", [string]$Label = "")
$env:PYTHONIOENCODING = "utf-8"
$py = "C:/gate/venv84a/Scripts/python.exe"
if ($Label -eq "") { $Label = $Stage }
$log = "C:/gate/log-$Driver-$Label.txt"
if ($Extra -eq "") {
    & $py "C:/gate/$Driver.py" $Stage 2>&1 | Out-File -Encoding utf8 $log
} else {
    & $py "C:/gate/$Driver.py" $Stage $Extra 2>&1 | Out-File -Encoding utf8 $log
}
Write-Output "exit=$LASTEXITCODE log=$log"
Get-Content $log
