# outpost-tools

Outpost Tools: one Windows application that holds Minecraft Java Edition tools in one window (a launcher, the world preview, a test server), shipped without any modpack. The product's intention, journeys and limits are written in the private pack repository's `specs/launcher.md`; this repository is the code. It is meant to be public on the maintainer's GitHub under GPL-3, and is not published yet.

## Rules that hold everywhere

- No pack of ours in here: no profile, no server address, no path of the maintainer's machine in code that ships. Test fixtures (`native/src/fake.rs`) are examples and may name sample profiles.
- Its name is Outpost Tools in the window title, the bar and the shortcuts. The game's name appears only in sentences about the game, and no Mojang trademark is used in the name or the icon.
- Interface sentences are English, kept in one catalog per form: the page's `text` object in `index.html`, and `native/src/text.rs`, which follows it line by line.
- Repository content is English. Conversations with the maintainer are French.
- Nothing is pushed or published without the maintainer's decision.

## Where things live

- `app/`: the shared api and the page launcher. `src/api.rs` routes each request to one file per part in `src/api/` (`profiles.rs`: Prism instances; `dev_server.rs`: the test server; `preview.rs`: the world preview; `key_value.rs`: the settings files). `src/system.rs` holds what is read from Windows (processes, the single-instance lock `Local\outpost-tools`, shared by both forms so they never run together). The binary (feature `window`) is a frameless tao window with a wry page, answering the page through its own protocol with no port; `--serve <port>` serves the page on localhost with no window, for `shots.ps1`.
- `index.html`: the page launcher's page, found by going up from the program to the first folder that holds it, and read at each load.
- `native/`: the native launcher, the default one. It is the world preview's window from mere-ui with the launcher's Home, calling `app`'s api in process. It depends on mere-ui by path (`../../mere-ui`), written in its `Cargo.toml` alone. Its tests (`cargo test`) use a fake api and a fixed date. They start no game, server or script, and write their pictures to `native/target/tmp`. The preview's pictures need a world preview out folder at `PREVIEW_DATA_DIR`, and are skipped without it.
- `docs/shots/`: the README's pictures, copied from `native/target/tmp`.

## The pack repository

Until profiles are data of Outpost Tools' own, the program drives one pack repository, given by `--repo <folder>`. Without `--repo`, it uses the first folder above the program that holds `scripts\common.ps1`, which is how a prod copy with Outpost Tools built inside the pack's snapshot finds it. From that folder it reads:

- `scripts/common.ps1`: `$DevServerDir`, `$PreviewDir`, `$PreviewExports`, `Find-Prism`, `Build-GlueMod`;
- `scripts/dev-server.ps1` and `scripts/rcon.ps1`, for the test server;
- `pack/`, for the world preview;
- `run/`, for the log (`run\launcher.log`) and the window's data (`run\app`).

The world preview's own repository is looked for beside the pack repository, or at `WORLD_PREVIEW_DIR`. It is the engine only, the generator that makes a world's maps, and stays a repository of its own so others can use it alone to generate worlds; every interface of the preview lives here (decided with the maintainer, 2026-11-15). Cutting these ties is the work listed in the README under "Not built yet".
