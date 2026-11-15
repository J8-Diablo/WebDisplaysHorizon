# Registry page: WebDisplays

What to type in the Horizon mods form (playhorizon.dev/mods/submit). Submit it after HCEF.

WebDisplays is one file: the native mod carries its blocks, items and assets inside. There is
no separate content mod to list.

## The file

| Field | Value |
|---|---|
| Kind | Native mod (.hnmod) |
| File | `webdisplays_native-0.1.0.hnmod` (a copy of `dist/webdisplays_native.hnmod`; with that name the form fills in the id and the version) |
| Id | `webdisplays_native` |
| Version | `0.1.0` |
| For Minecraft | `26.3` |
| What changed | `First release.` |

## Its page

| Field | Value |
|---|---|
| Name | WebDisplays |
| Licence | MIT |
| Categories | `technology`, `decoration` |
| Tags | `browser, screen, video, web, streaming` |
| Authors | montoyo, CinemaMod Group, J8-Diablo |
| Source code | https://github.com/J8-Diablo/WebDisplaysHorizon |
| Icon | `registry/webdisplays.png` (256 x 256 PNG) |
| Screenshots | optional, up to 8 (PNG or JPEG, 320x180 to 3840x2160, at most 1 MiB each) |

### Summary

```
Web browsers on screens in your world: videos, live streams, any site. Needs the HCEF mod.
```

### Description

```markdown
**WebDisplays** puts working web browsers on screens in your world: build a wall of screen
blocks, click it, type an address. Videos, live streams, any site.

**One file.** Its blocks, items, recipes, textures, sounds and translations (English and
French) come inside it: there is no separate content mod to install.

## It needs HCEF

The browser itself is another mod, **HCEF** (`hcef`). Install both and build: without HCEF
the launcher stops with "webdisplays_native requires missing mod hcef". Then, in the game,
open **Options > Mods > HCEF** and click the button once to download the browser (188 MB).

## What is in it

- Screens made of blocks, and screens traced with the stylus, flat or with rounded corners.
- Keyboard, remote control, redstone controller, linking tool, configurator, laser pointer,
  MinePad.
- One page over several screens, or one browser shown on several screens.
- Sound that follows the place: echo in a room, muffled behind a wall. Three speakers
  (tweeter, bass, full range).
- Light from the screens on what is around them.
- YouTube and Twitch addresses open as full-frame players, live streams included.

## What the mod does, and what it does not

It does:

- draw the pages on the screens, and send your clicks and typing to them;
- ask HCEF for browsers and give them the addresses players type: **those pages are loaded
  from the internet by the browser**, as in any browser, on each player's own computer;
- write two small files in the world's folder (`webdisplays/screens.bin` and
  `webdisplays/browsers.bin`: the list of screens and of shared browsers), and its content
  mod once into the instance's `.horizon-cache`;
- read and write the clipboard, only for Ctrl+C / Ctrl+V in its own windows.

It does not:

- open any network connection itself, download anything or start any program: that is
  HCEF's part, described on its page;
- read your game account or files outside the world's and the instance's folders;
- synchronize videos between players: each one plays the page for themself.

## Good to know

- Windows x64 only for now, and an early version.
- A world opened without the mod loses its WebDisplays blocks.

## Credits

A rewrite in Rust, for Horizon, of **WebDisplays**: the original mod by
[montoyo](https://github.com/montoyo/webdisplays) (released into the public domain) and its
continuation by the [CinemaMod Group](https://github.com/CinemaMod/webdisplays) (MIT). The
textures, models, sounds and translations come from it; its copyright and licence notices
are kept in the repository's `LICENSE`.
```

## Signature

| Field | Value |
|---|---|
| Public key | `RWT+RdlVVja6RRLkjF97dRvi8+FwzE1STLMSFnNkGBJxKcugEkXZkN7U` |
| Signature | `webdisplays_native-0.1.0.hnmod.minisig` (beside the file) |

Every later version must be signed with the same key. The secret key is the author's and is
not in this repository.

## Last box

"I made it, or I may publish it under this licence, and its source is at the address above.
I accept the mod policy and I am responsible for this project."
