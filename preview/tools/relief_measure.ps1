# Runs the relief's measure (examples/relief_measure.rs) on an out folder and reads, beside it, what the system
# says of the graphics card for that program: its dedicated memory and how busy its 3D engine is. Prints one row
# per part of the script and writes the whole as JSON.
#
#   .\relief_measure.ps1 -Data <out folder> -Out run\measure\blocks-2048.json [-Exe path] [-Detail blocks]
#
# The frame's interval is what the eye gets. The card's share of a frame is its busy time over the part, divided by
# the frames: the window waits for the screen's refresh, so the interval alone hides a card that is half idle.
param(
    [Parameter(Mandatory)][string]$Data,
    [Parameter(Mandatory)][string]$Out,
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\examples\relief_measure.exe'),
    [string]$Detail,
    [switch]$Windowed
)
$ErrorActionPreference = 'Stop'
$lines = [System.IO.Path]::GetTempFileName()
$arguments = @('--data', $Data, '--out', $lines)
if ($Detail) { $arguments += @('--detail', $Detail) }
if ($Windowed) { $arguments += '--windowed' }
$process = Start-Process -FilePath $Exe -ArgumentList $arguments -PassThru -WindowStyle Normal
$id = $process.Id
$samples = @()
$cpuBefore = $null
while (-not $process.HasExited) {
    try {
        # One second of the counters: the engine's share is a mean over it.
        $c = Get-Counter -Counter @("\GPU Process Memory(pid_${id}_*)\Dedicated Usage", "\GPU Engine(pid_${id}_*engtype_3D)\Utilization Percentage") -SampleInterval 1 -ErrorAction Stop
        $at = [DateTimeOffset]::new($c.Timestamp).ToUnixTimeMilliseconds()
        $memory = ($c.CounterSamples | Where-Object Path -like '*dedicated usage' | Measure-Object CookedValue -Sum).Sum
        $busy = ($c.CounterSamples | Where-Object Path -like '*utilization percentage' | Measure-Object CookedValue -Sum).Sum
        $process.Refresh()
        $cpu = $process.TotalProcessorTime.TotalMilliseconds
        $samples += [pscustomobject]@{ at = $at; memory = $memory; busy = $busy; cpu = $cpu; ram = $process.WorkingSet64 }
    } catch {
        Start-Sleep -Milliseconds 200
    }
}
$records = Get-Content $lines | ForEach-Object { $_ | ConvertFrom-Json }
Remove-Item $lines
$parts = foreach ($r in $records | Where-Object part) {
    # A sample covers the second that ends at its time: kept when that second lies inside the part.
    $inside = @($samples | Where-Object { $_.at - 1000 -ge $r.from_ms -and $_.at -le $r.to_ms })
    $busy = if ($inside) { ($inside | Measure-Object busy -Average).Average } else { $null }
    $seconds = ($r.to_ms - $r.from_ms) / 1000
    [pscustomobject]@{
        part          = $r.part
        frames        = $r.frames
        interval_ms   = $r.interval.mean
        interval_p95  = $r.interval.p95
        interval_max  = $r.interval.max
        processor_ms  = $r.processor.mean
        shown_ms      = $r.shown.mean
        card_busy_pct = if ($null -ne $busy) { [math]::Round($busy, 1) } else { $null }
        # The card's busy time per frame drawn.
        card_ms       = if ($null -ne $busy) { [math]::Round($busy / 100 * $seconds * 1000 / $r.frames, 2) } else { $null }
        each          = $r.each
    }
}
$meshes = @($records | Where-Object { $null -ne $_.mesh })
$result = [pscustomobject]@{
    data         = $Data
    detail       = $Detail
    samples      = ($records | Where-Object pid).samples
    built        = @($records | Where-Object built)
    build_ms     = ($records | Where-Object { $null -ne $_.build_ms }).build_ms
    start_ms     = ($records | Where-Object { $null -ne $_.start_ms }).start_ms
    meshes       = $meshes
    images       = @($records | Where-Object { $null -ne $_.image })
    card_memory_mb = [math]::Round((($samples | Measure-Object memory -Maximum).Maximum) / 1MB, 1)
    ram_mb       = [math]::Round((($samples | Measure-Object ram -Maximum).Maximum) / 1MB, 1)
    parts        = @($parts)
}
New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
$result | ConvertTo-Json -Depth 6 | Set-Content $Out
"{0}: {1} samples a side, card memory {2} MB, meshes built in {3} ms" -f $Data, $result.samples, $result.card_memory_mb, $result.build_ms
$parts | Format-Table part, frames, interval_ms, interval_p95, interval_max, processor_ms, shown_ms, card_busy_pct, card_ms -AutoSize
