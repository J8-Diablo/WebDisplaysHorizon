# Registry page: WebDisplays Pack

What to type in the Horizon mods form (playhorizon.dev/mods/submit). Submit it **last**: a
modpack may only list versions that are already in the registry, with the same SHA-256.

The pack pins the three files `tools/pack.py` writes into `dist/` (HCEF's and this
repository's): submit those very files. Packing is reproducible, so the same sources give the
same SHA-256; if a source changes, update `registry/pack/horizon-modpack.toml` and rebuild
the `.hpack` (a zip holding that one file).

| Field | Value |
|---|---|
| Kind | Modpack (.hpack) |
| File | `registry/webdisplays_pack-0.1.0.hpack` |
| Id | `webdisplays_pack` |
| Version | `0.1.0` |
| Game | `26.3` |
| Name | WebDisplays Pack |
| Authors | J8-Diablo |
| License | MIT |
| Source | https://github.com/J8-Diablo/WebDisplaysHorizon |
| Categories | `technology`, `lightweight` |
| Tags | `browser`, `screen`, `video`, `web` |
| Icon | `registry/webdisplays.png` (256 x 256) |

## Summary

```
WebDisplays in one install: the blocks and items, the code and the HCEF browser.
```

## Description

```markdown
**WebDisplays Pack** installs the three mods WebDisplays for Horizon is made of, and nothing
else:

- **WebDisplays**: the blocks and items.
- **WebDisplays Native**: the code.
- **HCEF**: the browser.

Two of them are native mods: the launcher asks you to trust them and builds the game, which
takes a while the first time. Then, in the game, open **Options > Mods > HCEF** and click
the button once to download the browser (188 MB).

After that: build a wall of screen blocks, click it, type an address.

Windows x64 only for now, and an early version.
```

## What changed

```markdown
First release: HCEF 0.1.0, WebDisplays Native 0.1.0, WebDisplays 0.1.0.
```
