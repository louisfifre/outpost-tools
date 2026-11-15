<#
.SYNOPSIS
Takes pictures of the launcher page without showing a window, so that a change can be looked at.

.DESCRIPTION
Starts the launcher program without its window (launcher --serve) on its own port, then drives a headless Chromium browser over the
DevTools protocol: open a page, wait until the page says it is ready, save a PNG. The owner's
launcher and its window are never touched.

A page says it is ready by setting data-ready="1" on its root element, once everything it had to
fetch is drawn. Taking the picture when the page loads is not enough: the data arrives after.

It lives with the launcher it pictures; the shared helpers are in scripts/common.ps1. The world preview has its own capture script, in its own repository.

.PARAMETER Shots
One hashtable per picture. Name is the file name. Path defaults to /. Size defaults to
'1100,760'. Scheme is 'dark' or 'light'. Eval is JavaScript run once the page is ready. Wait is
how long to let the page settle after that, in milliseconds. Stay keeps the page of the shot before
instead of opening one. Until is a JavaScript condition to wait for before the picture.
Scale is the pixel density (2 gives a picture twice as fine, for an illustration). Clip is a CSS
selector: the picture holds that element only, with Pad pixels around it. Url opens any address
instead of the launcher page, a local file included.
@{ Name = 'launcher'; Scale = 2 }, @{ Name = 'light'; Scheme = 'light' }
Without -Shots, the usual set is taken.

.PARAMETER OutDir
Where the pictures land. run\shots when omitted (not versioned).
#>
param(
    [hashtable[]]$Shots,
    [string]$OutDir
)

. (Join-Path (Split-Path $PSScriptRoot -Parent) 'scripts\common.ps1')

$Port = 25581
$DebugPort = 9333
if (-not $OutDir) { $OutDir = Join-Path $RepoRoot 'run\shots' }
New-Item -ItemType Directory -Force $OutDir | Out-Null

if (-not $Shots) {
    $Shots = @(
        @{ Name = 'launcher' }
        @{ Name = 'launcher-light'; Scheme = 'light' }
    )
}

$browserExe = Find-AppBrowser
if (-not $browserExe) { throw 'No Chromium browser found (Brave or Edge).' }

$script:callId = 0
function Send-Cdp($socket, [string]$method, $params = @{}) {
    # One DevTools call: send it, then read messages until its own answer comes back.
    $script:callId++
    $mine = $script:callId
    $json = @{ id = $mine; method = $method; params = $params } | ConvertTo-Json -Depth 6 -Compress
    $bytes = [Text.Encoding]::UTF8.GetBytes($json)
    $socket.SendAsync([ArraySegment[byte]]::new($bytes), 'Text', $true, [Threading.CancellationToken]::None).Wait()
    $buffer = [byte[]]::new(1MB)
    while ($true) {
        $stream = [IO.MemoryStream]::new()
        do {
            $got = $socket.ReceiveAsync([ArraySegment[byte]]::new($buffer), [Threading.CancellationToken]::None).Result
            $stream.Write($buffer, 0, $got.Count)
        } until ($got.EndOfMessage)
        $reply = [Text.Encoding]::UTF8.GetString($stream.ToArray()) | ConvertFrom-Json
        if ($reply.id -eq $mine) { return $reply.result }
    }
}

# The launcher program without its window: the page and its answers on the port, the owner's window untouched.
$server = Start-Process (Get-LauncherApp) -PassThru -ArgumentList '--serve', $Port
# Its own profile folder: the owner's browser, its tabs and its sign-ins stay out of it.
$browser = Start-Process $browserExe -PassThru -ArgumentList '--headless=new', "--remote-debugging-port=$DebugPort",
    "--user-data-dir=`"$(Join-Path $ToolsDir 'capture-profile')`"", '--hide-scrollbars', '--enable-unsafe-swiftshader', 'about:blank'
try {
    $target = $null
    foreach ($try in 1..60) {
        try { $target = Invoke-RestMethod "http://localhost:$DebugPort/json/new?about:blank" -Method Put; break }
        catch { Start-Sleep -Milliseconds 250 }
    }
    if (-not $target) { throw 'The browser did not open its DevTools port.' }
    foreach ($try in 1..60) {
        try { $null = Invoke-WebRequest "http://localhost:$Port/api/ping" -UseBasicParsing -TimeoutSec 2; break }
        catch { Start-Sleep -Milliseconds 250 }
    }
    $socket = [Net.WebSockets.ClientWebSocket]::new()
    $socket.ConnectAsync([Uri]$target.webSocketDebuggerUrl, [Threading.CancellationToken]::None).Wait()
    $null = Send-Cdp $socket 'Page.enable'

    foreach ($shot in $Shots) {
        $path = if ($shot.Path) { $shot.Path } else { '/' }
        $width, $height = ($(if ($shot.Size) { $shot.Size } else { '1100,760' })).Split(',')
        $scheme = if ($shot.Scheme) { $shot.Scheme } else { 'dark' }
        $scale = if ($shot.Scale) { [double]$shot.Scale } else { 1 }
        $null = Send-Cdp $socket 'Emulation.setDeviceMetricsOverride' @{ width = [int]$width; height = [int]$height; deviceScaleFactor = $scale; mobile = $false }
        $null = Send-Cdp $socket 'Emulation.setEmulatedMedia' @{ features = @(@{ name = 'prefers-color-scheme'; value = $scheme }) }
        $url = if ($shot.Url) { $shot.Url } else { "http://localhost:$Port$path" }
        if (-not $shot.Stay) { $null = Send-Cdp $socket 'Page.navigate' @{ url = $url } }
        # Waits for the page to say it is drawn, or for Until.
        $ready = $false
        foreach ($try in 1..480) {
            Start-Sleep -Milliseconds 250
            $condition = if ($shot.Until) { $shot.Until } else { "document.documentElement.dataset.ready === '1'" }
            $state = Send-Cdp $socket 'Runtime.evaluate' @{ expression = $condition }
            if ($state.result.value) { $ready = $true; break }
        }
        if ($shot.Eval) { $null = Send-Cdp $socket 'Runtime.evaluate' @{ expression = $shot.Eval; awaitPromise = $true } }
        Start-Sleep -Milliseconds $(if ($shot.Wait) { [int]$shot.Wait } else { 300 })
        $picture = @{ format = 'png' }
        if ($shot.Clip) {
            $pad = [int]$shot.Pad
            $found = (Send-Cdp $socket 'Runtime.evaluate' @{ expression = "JSON.stringify(document.querySelector('$($shot.Clip)')?.getBoundingClientRect() ?? null)" }).result.value
            if (-not $found) { throw "Nothing on the page matches the Clip selector $($shot.Clip)" }
            $box = $found | ConvertFrom-Json
            $picture.clip = @{ x = $box.x - $pad; y = $box.y - $pad; width = $box.width + 2 * $pad; height = $box.height + 2 * $pad; scale = 1 }
        }
        $png = Send-Cdp $socket 'Page.captureScreenshot' $picture
        $file = Join-Path $OutDir "$($shot.Name).png"
        [IO.File]::WriteAllBytes($file, [Convert]::FromBase64String($png.data))
        Write-Host "$file$(if (-not $ready) { '  (the page never said it was ready)' })"
    }
} finally {
    Stop-Process -Id $browser.Id -Force -ErrorAction SilentlyContinue
    Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
}
