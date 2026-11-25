# Outpost Tools

<!-- PLACEHOLDER: the opening picture, taken in the game by the maintainer. A pillager outpost, with the world
     stretching far behind it. Replace this block with:
     ![A pillager outpost, with the world stretching far behind it](docs/shots/outpost.png) -->
> **[Picture to come]** A pillager outpost in the game, with the world stretching far behind it.

Outpost Tools is one Windows application that holds Minecraft Java Edition tools in one window:

- **Home**: your profiles, each with its Play button, and your servers, each with Start or Stop. The account you play with is shown in the bar;
- a **profile's page**: its mods (updates, remove with Undo, add one from Modrinth or from a file), Fabric, the game version, memory and Java arguments;
- a **server's page**: the profile and world it was made from, and the Minecraft EULA, accepted by you;
- the **Java set-up** at the first start, when the game's Java is missing;
- the **world preview**: a map of a world drawn before anyone explores it, from the game's own generator.

A bar at the top switches between Home and the preview, and closing the window stops everything it started. It ships with no modpack: you add the packs you play.

> **Work in progress.** Today Outpost Tools runs on its maintainer's machine, built from source, next to the modpack it was made for. It is not yet something you can download and run. [Not built yet](#not-built-yet) lists what is missing.

## Screenshots

| | Dark | Light |
| --- | --- | --- |
| Home | ![Home, dark](docs/shots/home-dark.png) | ![Home, light](docs/shots/home-light.png) |
| A profile | ![A profile's page, dark](docs/shots/profile-dark.png) | ![A profile's page, light](docs/shots/profile-light.png) |
| Add a mod | ![Add a mod, dark](docs/shots/add-mod-dark.png) | ![Add a mod, light](docs/shots/add-mod-light.png) |
| A server | ![A server's page, dark](docs/shots/server-dark.png) | ![A server's page, light](docs/shots/server-light.png) |
| Set-up | ![The Java set-up, dark](docs/shots/setup-dark.png) | ![The Java set-up, light](docs/shots/setup-light.png) |
| World preview | ![World preview, dark](docs/shots/preview-dark.png) | ![World preview, light](docs/shots/preview-light.png) |

The pictures of the screens come from the app's own tests (`cargo test --test readme_shots` in `native/`), drawn offscreen from a fake api: sample profiles, a sample account, no real data. The world preview's pictures are older and need a generated world to redraw. The window follows Windows' light or dark setting.

## How it works

- It is native Rust, drawn with [mere-ui](#what-it-needs-today), its maintainer's interface toolkit (Direct2D, no web view). The world preview's window lives here (`preview/`), and its maps come from the `world-preview` repository's engine, which the app calls as a Rust library (`world_preview::run`), in its own process.
- The game starts through [Prism Launcher](https://prismlauncher.org/), which keeps the Microsoft sign-in. Outpost Tools only gives Prism commands: you never open Prism to play. The account in the bar is read from Prism, with no sign-in of its own.
- Each profile is a Prism instance. Its settings (memory, Java arguments, what it opens on: the menu, a world or a server) are written straight into Prism's own files, so the two never disagree.
- Mods are read from the profile's folder. Updates, search and Add ask Modrinth, for the profile's game version and Fabric. Remove keeps the file aside until Undo runs out.
- Memory is set on a bar with a minimum and a maximum, in half gigabytes, against the PC's memory.
- A server runs as a Java process that Outpost Tools starts and stops. `eula.txt` is written only when you press Accept on the server's page.
- At the first start without Java 25, the set-up offers to download it from Adoptium into Outpost Tools' folder.
- An older form of the launcher, a web page in a frameless window of its own (`app/` and `index.html`), still exists. Both forms share one api (`app/src/api/`) and never run together.

## What it needs today

The app builds from a clone of this repository alone: cargo fetches mere-ui and the `world-preview` engine from GitHub, each pinned by a tag in the `Cargo.toml` files.

To build and run Outpost Tools you need:

- Windows, PowerShell 7 and Rust (`rustup`, stable);
- the `world-preview` repository beside the pack or at `WORLD_PREVIEW_DIR`, for the preview tool: the app builds its mod at run time from that checkout;
- Prism Launcher, installed with Scoop;
- a **pack repository**, given to the program with `--repo <folder>`. That is where it reads what it drives today: `scripts/common.ps1` (the server and preview folders, and where Prism is), `scripts/dev-server.ps1` and `scripts/rcon.ps1` (the test server), and `pack/` (the modpack the world preview reads).

That last point is the main gap. Profiles and folders should be data in Outpost Tools' own folder, not the scripts of one pack.

## Not built yet

What stands between today and the goal (download one exe, double-click, play):

- **The self-installing exe.** It would put itself in its own folder, add its Start menu shortcut and update itself from this repository's releases.
- **Portable Prism.** Prism Launcher would be downloaded in its portable form and kept up to date by Outpost Tools, so nobody installs it by hand. The set-up names Prism and Sign in as steps, but does only Java.
- **Profiles as data.** Profiles would be added from a modpack file and kept in Outpost Tools' folder, instead of the paths a pack's scripts write.
- **The Rust port.** What runs in PowerShell today (the pack's sync, the test server) would move to Rust, as the preview's generation already has, so the PC needs nothing else.
- **Signing.** The exe is not signed: Windows warns once on the first download, and Smart App Control blocks it. Signing names the publisher, but the warning stays until the app has a reputation.

On Home and a profile's page:

- **Update counts.** "N updates" on Home appears once a profile's page was opened in this run. It is not kept between runs, and nothing checks Modrinth in the background.
- **Order.** Home lists the last played first, so a profile never played comes last.
- **New profile** and **New server** do nothing yet.
- **Modrinth.** Answers are kept five minutes, with no Refresh, so a new version can show late. A mod whose newest version is for another game or loader shows no number on its Add line. Not yet tried against the live site.
- **Memory bar.** One focus stop serves both ends (Up and Down switch the end). Screen readers are not supported.

On servers:

- **Never run for real.** Start, Stop and the folder dialog are tested with a fake runner only. They need one real try.
- **A server started elsewhere** (an earlier run, or by hand) shows as running but cannot be stopped from the window. Closing Outpost Tools leaves a running server running.
- **Failed starts.** Only refusals before the process starts are said (EULA, Java missing, port taken). A server that dies after starting goes back to stopped, with no sentence and no log link.
- **Java missing** is said, but does not lead to the set-up yet.
- **Settings.** After Accept, the server's page is still a placeholder form. Memory is 6 GB unless the profile says otherwise.

Also not built: the bar's cube is a placeholder icon, and the taskbar may group the window under `outpost-tools` until the exe carries a version resource.

## Building

```powershell
cd native; cargo build --release     # the app: native\target\release\outpost-tools.exe
cd native; cargo test                # its tests; the pictures land in native\target\tmp
cd app; cargo build --release        # the page launcher: app\target\release\launcher.exe
```

Run either with `--repo <pack repository>`. `outpost-tools --shot out.png --tool home|preview --theme dark|light` draws one frame offscreen (Home or the preview) from the fake api, with no window. The world preview's pictures, and the tests that draw them, read the world of a preview's out folder named by `--data` or the `PREVIEW_DATA_DIR` environment variable; without one they are skipped. `shots.ps1` takes pictures of the page launcher in a headless Brave or Edge.

`preview/` (the world preview's window) and `relief/` (its relief in 3D) build and test the same way, each in its own folder.

### Working on mere-ui at the same time

To try a change of mere-ui before it is tagged, point cargo at a local checkout in a `.cargo/config.toml` at the root of this repository, which git ignores:

```toml
[patch."https://github.com/louisfifre/mere-ui"]
mere-base = { path = "../mere-ui/crates/mere-base" }
mere-layout = { path = "../mere-ui/crates/mere-layout" }
mere-ui = { path = "../mere-ui/crates/mere-ui" }
mere-win32 = { path = "../mere-ui/crates/mere-win32" }
mere-png = { path = "../mere-ui/crates/mere-png" }
```

The paths are relative to this repository's root, or absolute. While the patch is there, cargo rewrites the `Cargo.lock` of each crate you build to the local copy: **do not commit a `Cargo.lock` changed by the patch**. Remove the file, then `git checkout -- <crate>/Cargo.lock`, before committing.

## What is where

| Path | What it holds |
| --- | --- |
| `native/` | `outpost-tools.exe`, the default form: the world preview's window with Home and its pages, calling `app`'s api as a library. `src/fake.rs` is the fake api its tests and pictures use. |
| `native/src/pages/` | The pages over Home: a profile, add a mod, a server, the set-up. |
| `preview/` | The world preview's window, drawn with mere-ui, over the `world-preview` engine; also a program of its own. |
| `relief/` | The world preview's relief in 3D, in a window or as a PNG, from a real out folder. |
| `app/` | The api every form shares (`src/api/`: profiles, mods, Modrinth, servers, Java set-up, world preview, settings files), and the page launcher's program (tao and wry). |
| `index.html` | The page launcher's page, read from disk at each load. |
| `shots.ps1` | Pictures of the page launcher without a window. |
| `docs/shots/` | The pictures of this README. |
| `THIRD-PARTY.md` | Third-party work this repository carries (the icons). |

## Licence

Outpost Tools is under the [GNU General Public License v3.0](LICENSE). Third-party parts keep their own licences: see [THIRD-PARTY.md](THIRD-PARTY.md). Minecraft is a trademark of Mojang. Outpost Tools is not affiliated with Mojang or Microsoft.
