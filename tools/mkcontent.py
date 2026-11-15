"""Builds the WebDisplays content mod for Horizon from the Java mod's resources.

usage: python -I mkcontent.py <WebDisplays Java repo> [content mod folder, default ../content]

The content mod is the half of WebDisplays with no code: blocks, items, their models,
textures, recipes, advancements, sounds and languages. Everything under `assets/` and
`data/` of the output is written by this script and removed first, so nothing there is
edited by hand. `horizon-mod.toml` is written too. `check_content.py` checks the result.

Dependencies: none (the speakers' textures are written as PNG with zlib and struct).
"""
import json
import math
import os
import re
import shutil
import struct
import sys
import zlib

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
repo = sys.argv[1]
out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(HERE, "content")
MOD = "webdisplays"
VERSION = "0.1.0"

for stale in ("assets", "data"):
    shutil.rmtree(os.path.join(out, stale), ignore_errors=True)


def write(rel, value):
    path = os.path.join(out, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write("\n")


def write_text(rel, text):
    path = os.path.join(out, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


def read(rel):
    """A JSON file of the Java mod's resources, with or without a byte order mark."""
    with open(os.path.join(repo, "common/src/main/resources", rel), encoding="utf-8-sig") as f:
        return json.load(f)


def ident(name):
    return "%s:%s" % (MOD, name)


write_text("horizon-mod.toml", '''id = "%s"
name = "WebDisplays"
version = "%s"
description = "Web browsers on screens in the world."
authors = ["montoyo", "CinemaMod Group", "J8-Diablo"]
# A content mod: blocks, items and their assets, no code (API 0.2). The behaviour (the
# browsers, the screens' logic, the sound) is the native mod's, in ../native.
api = "0.2"
side = "both"
capabilities = ["registries"]
''' % (MOD, VERSION))

# ---- assets taken from the Java mod as they are. Left behind: the Java mod's own GLSL,
# its block states (written below: they name properties and loaders Horizon has not), the
# pages and scripts the browser process serves (host/), the languages and sounds.json
# (written below, without what was dropped), and what belonged to the parts that were
# dropped: the ComputerCraft and OpenComputers interfaces, the file server, the redstone
# and GPS upgrades, the failed extension card, and the advancement icons no advancement
# uses (the Java mod never registered them as items).
SKIP_DIRS = {"shaders", "blockstates", "html", "js", "lang", ".cache"}
DROPPED = re.compile(r"(^|/)(ccinterface|ocinterface|cci_|oci_|server|upgrade_gps|upgrade_redinput|upgrade_redoutput"
                     r"|craftcomp_badextcard|badextcard|gps|input|output|advicon_)[a-z0-9_]*\.(json|png|mcmeta|ogg)$")
base = os.path.join(repo, "common/src/main/resources/assets", MOD)
skipped = []
for folder, _, files in os.walk(base):
    rel = os.path.relpath(folder, base).replace(os.sep, "/")
    if rel.split("/")[0] in SKIP_DIRS:
        continue
    for name in files:
        source = os.path.join(folder, name)
        target = "assets/%s/%s" % (MOD, name) if rel == "." else "assets/%s/%s/%s" % (MOD, rel, name)
        if target == "assets/%s/sounds.json" % MOD:
            continue
        if not re.fullmatch(r"[a-z0-9_./-]+", target) or DROPPED.search(target):
            skipped.append(target)
            continue
        if name.endswith((".json", ".mcmeta")):
            # Written again without the byte order mark some files start with.
            try:
                with open(source, encoding="utf-8-sig") as f:
                    write(target, json.load(f))
            except ValueError:
                skipped.append(target)
        else:
            path = os.path.join(out, target)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            shutil.copyfile(source, path)

# The Java screen model names a Forge model loader; the screen's models are written below.
# `screen_item` is a second copy of the item's model (`item/screen`) that nothing names.
for name in ("screen", "screen_item"):
    os.remove(os.path.join(out, "assets", MOD, "models/block/%s.json" % name))

# ---- blocks
DIRECTIONS = ["down", "up", "north", "south", "west", "east"]
FULL = [[0, 0, 0, 1, 1, 1]]
# Stone, as the Java screen: needs a pickaxe.
STONE = {"hardness": 1.5, "explosion_resistance": 6.0, "sounds": "minecraft:stone", "map_color": 11,
         "light": 0, "requires_tool": True}
# The Java keyboard, controllers and traced screen: stone with strength(1.5, 10).
TOUGH = dict(STONE, explosion_resistance=10.0)
SCREENS_TAG = "#%s:screens" % MOD

blocks = {}

# A screen block joins its neighbours: the chorus plant's six `down`..`east` properties are
# kept up to date by the game itself (true where the neighbour is in the tag), and the block
# state below draws each face's bezel from them. `shapes` gives back the full cube.
#
# `screen` is the block players place. `screen_core` is the same block with a block entity:
# the native mod swaps one block of a screen for it when the screen is made, and it holds
# the screen's data (a block entity cannot be given to some blocks of an id and not others).
for name, item in (("screen", True), ("screen_core", False)):
    blocks[name] = {
        "template": "minecraft:chorus_plant",
        "connects_to": SCREENS_TAG,
        "properties": STONE,
        "shapes": {"default": FULL},
        "item": item,
    }

# The keyboard is two thin blocks side by side, `kb_left` (which holds the block entity in
# the Java mod) and `kb_right`. Glazed terracotta gives them a horizontal `facing` and no
# behaviour. It is placed facing the player, so `facing` here is the side the typist stands
# on: the OPPOSITE of the Java blocks' FACING, which is where the player looked (the Java
# item sets it itself). Seen from above, the right half is anticlockwise of `facing`
# (facing south, the typist to the south: the right half is to the east).
#
# The 13 x 1 x 16 pixel slab leaves three pixels free on the far side from the typist
# (KeyboardBlockLeft.KEYBOARD_AABBS); outline and collision are the same, as in Java.
KEYBOARD_SHAPES = {
    "south": [[0, 0, 3 / 16, 1, 1 / 16, 1]],
    "north": [[0, 0, 0, 1, 1 / 16, 13 / 16]],
    "east": [[3 / 16, 0, 0, 1, 1 / 16, 1]],
    "west": [[0, 0, 0, 13 / 16, 1 / 16, 1]],
}
for name in ("kb_left", "kb_right"):
    blocks[name] = {
        "template": "minecraft:white_glazed_terracotta",
        # Not a full cube: the faces round it are drawn (the Java getOcclusionShape is empty).
        "properties": dict(TOUGH, occludes=False),
        "shapes": {"property": "facing", "values": KEYBOARD_SHAPES},
        # The `keyboard` item (below) places `kb_left`; the native mod adds `kb_right`.
        "item": False,
    }

# The remote controller and the redstone controller: plain full cubes without a facing
# (PeripheralBlock). Their block entities and what a right click does are the native mod's.
for name in ("rctrl", "redctrl"):
    blocks[name] = {"template": "minecraft:stone", "properties": TOUGH, "item": True}

# The blocks a traced (free-form) screen is made of: PolyScreenBlock. Invisible, since the
# native mod draws the whole sheet as one mesh, but a full cube to walk into and to click.
# The barrier is that block in vanilla: nothing drawn, full collision, light and skylight
# pass, nothing replaces it and no piston moves it (PushReaction.BLOCK in Java). It also
# brings `waterlogged`, which only a creative player's bucket can set and the native mod
# leaves false. `render_shape` and `occludes` say again what the barrier already is, so
# the block stays invisible should its template change. No item, and no drops (below).
#
# `polyscreen_core` is to `polyscreen` what `screen_core` is to `screen`: the one block of
# a traced screen that carries the block entity.
for name in ("polyscreen", "polyscreen_core"):
    blocks[name] = {
        "template": "minecraft:barrier",
        "properties": dict(TOUGH, render_shape="INVISIBLE", occludes=False),
        "shapes": {"default": FULL},
        "item": False,
    }

# The speakers (new, not in the Java mod): full cubes turned in six directions. The
# dispenser gives the `facing` wanted: set when the block is placed, to the opposite of
# where the player looks, so the front of a speaker faces whoever places it, floor and
# ceiling included. A block named otherwise than minecraft:dispenser dispenses nothing and
# has no dispenser block entity; what it keeps of the dispenser is `triggered`, true while
# the block (or the one above it) is powered by redstone, and a right click that looks for
# a menu to open: there is none, and the game says so ("... is not available yet") unless
# the native mod takes the click first (native_right_click_block). The item loses the
# dispenser's (empty) container component.
SPEAKERS = ("speaker_tweeter", "speaker_bass", "speaker_mix")
for name in SPEAKERS:
    blocks[name] = {
        "template": "minecraft:dispenser",
        "properties": STONE,
        "item": True,
        "components": {"!minecraft:container": {}},
    }

for name, block in blocks.items():
    write("data/%s/horizon/block/%s.json" % (MOD, name), block)

write("data/%s/tags/block/screens.json" % MOD, {"values": [ident("screen"), ident("screen_core")]})
write("data/%s/tags/block/speakers.json" % MOD, {"values": [ident(name) for name in SPEAKERS]})
write("data/minecraft/tags/block/mineable/pickaxe.json",
      {"values": [ident(name) for name in sorted(blocks)]})

# What each block drops, unless an explosion took it. A core drops a plain screen block;
# the keyboard's left half drops the keyboard and its right half nothing (the Java tables
# gave one each, and the Java blocks then removed the other half without drops); a traced
# screen's blocks drop nothing. 26.3 writes a pool's condition as `condition`, with a
# `type`: the older `conditions` list is not read.
DROPS = {"screen": "screen", "screen_core": "screen", "kb_left": "keyboard", "kb_right": None,
         "rctrl": "rctrl", "redctrl": "redctrl", "polyscreen": None, "polyscreen_core": None}
DROPS.update((name, name) for name in SPEAKERS)
for name, drop in DROPS.items():
    table = {"type": "minecraft:block"}
    if drop:
        table["pools"] = [{
            "condition": {"type": "minecraft:survives_explosion"},
            "entries": [{"type": "minecraft:item", "name": ident(drop)}],
            "rolls": 1,
        }]
    table["random_sequence"] = "%s:blocks/%s" % (MOD, name)
    write("data/%s/loot_table/blocks/%s.json" % (MOD, name), table)

# ---- the screen's bezel. Each face shows one of sixteen textures, `screen0` to `screen15`:
# a bar on each edge with no screen block beyond it (bottom 1, right 2, top 4, left 8, as
# the texture is seen), so that a wall of screens has one frame round the whole. This is
# the Java mod's baked model (ScreenBaker), written as a multipart block state.
#
# For each face, where the edges of its texture point in the world, with the model's
# default UVs (the texture upright on the four walls; on top its upper edge to the north;
# underneath its upper edge to the south).
EDGES = {
    "north": {"top": "up", "bottom": "down", "right": "west", "left": "east"},
    "south": {"top": "up", "bottom": "down", "right": "east", "left": "west"},
    "west": {"top": "up", "bottom": "down", "right": "south", "left": "north"},
    "east": {"top": "up", "bottom": "down", "right": "north", "left": "south"},
    "up": {"top": "north", "bottom": "south", "right": "east", "left": "west"},
    "down": {"top": "south", "bottom": "north", "right": "east", "left": "west"},
}
BAR = {"bottom": 1, "right": 2, "top": 4, "left": 8}

multipart = []
for face in DIRECTIONS:
    for mask in range(16):
        # A face against another screen block is never seen: nothing is drawn for it.
        when = {face: "false"}
        for edge, bit in BAR.items():
            # A bar where there is no neighbour: the property is false.
            when[EDGES[face][edge]] = "false" if mask & bit else "true"
        model = "%s:block/screen/%s_%d" % (MOD, face, mask)
        multipart.append({"when": when, "apply": {"model": model}})
        write("assets/%s/models/block/screen/%s_%d.json" % (MOD, face, mask), {
            "textures": {"face": "%s:block/screen%d" % (MOD, mask), "particle": "%s:block/screen15" % MOD},
            "elements": [{
                "from": [0, 0, 0],
                "to": [16, 16, 16],
                "faces": {face: {"texture": "#face", "cullface": face}},
            }],
        })
for name in ("screen", "screen_core"):
    write("assets/%s/blockstates/%s.json" % (MOD, name), {"multipart": multipart})

# ---- the other blocks' states. A variant key names some of a block's properties: the
# ones it leaves out (`triggered`, `waterlogged`) change nothing that is drawn.


def turned(model, y=0, x=0):
    variant = {"model": ident(model)}
    if x:
        variant["x"] = x
    if y:
        variant["y"] = y
    return variant


# The Java keyboard models are drawn for a player looking north, who stands to the south:
# `facing=south` here, and a quarter turn for each side after it.
QUARTER = {"south": 0, "west": 90, "north": 180, "east": 270}
for name in ("kb_left", "kb_right"):
    write("assets/%s/blockstates/%s.json" % (MOD, name), {
        "variants": {"facing=" + facing: turned("block/" + name, y) for facing, y in QUARTER.items()}})

for name in ("rctrl", "redctrl"):
    write("assets/%s/blockstates/%s.json" % (MOD, name), {"variants": {"": turned("block/" + name)}})

# Nothing is drawn for a traced screen's blocks; the model gives the particles of a broken
# one, as the barrier's does (the Java model had no texture at all).
write("assets/%s/models/block/polyscreen.json" % MOD, {"textures": {"particle": ident("block/screen15")}})
for name in ("polyscreen", "polyscreen_core"):
    write("assets/%s/blockstates/%s.json" % (MOD, name), {"variants": {"": turned("block/polyscreen")}})

# A speaker: the driver on the front (the model's north), the cabinet on the five other
# faces; turned as the observer's model is, front to `facing`.
FRONT_TO = {"north": {}, "east": {"y": 90}, "south": {"y": 180}, "west": {"y": 270},
            "up": {"x": 270}, "down": {"x": 90}}
for name in SPEAKERS:
    write("assets/%s/models/block/%s.json" % (MOD, name), {
        "parent": "minecraft:block/orientable",
        "textures": {"front": ident("block/" + name), "side": ident("block/speaker_side"),
                     "top": ident("block/speaker_side")},
    })
    write("assets/%s/blockstates/%s.json" % (MOD, name), {
        "variants": {"facing=" + facing: turned("block/" + name, **turn) for facing, turn in FRONT_TO.items()}})

# ---- the speakers' textures, drawn here: 16 x 16, in the greys of the screen's bezel
# (`screen15.png`: a #181818 frame round a #2f2f2f panel), each front readable from a
# distance: an oval horn round a small dome (tweeter), one large cone (bass), a small dome
# over a cone (full range). Every pixel is computed, with no randomness, so each run
# writes the same files.


def png(rel, width, height, pixel):
    """Writes an RGBA PNG whose pixel (x, y) is pixel(x, y)."""
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    rows = b"".join(b"\0" + b"".join(bytes(pixel(x, y)) for x in range(width)) for y in range(height))
    path = os.path.join(out, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


BEZEL, PANEL = 0x18, 0x2f


def rgb(r, g, b):
    return tuple(max(0, min(255, int(round(v)))) for v in (r, g, b)) + (255,)


def grey(v):
    return rgb(v, v, v)


def speck(x, y, salt):
    """A fixed pseudo-random byte per pixel, for the bezel's worn pixels."""
    n = (x * 73856093) ^ (y * 19349663) ^ (salt * 83492791)
    n = (n ^ (n >> 13)) * 1274126177
    return (n ^ (n >> 16)) & 0xFF


def cabinet(x, y, salt):
    """The speakers' box: the screen's bezel round a panel with a bevel and four screws."""
    if x in (0, 15) or y in (0, 15):
        n = speck(x, y, salt)
        return grey(BEZEL if n > 70 else (0x13, 0x14, 0x10, 0x0c)[n % 4])
    if x in (2, 13) and y in (2, 13):
        return grey(0x5a)
    if x == 1 or y == 1:
        return grey(0x39)
    if x == 14 or y == 14:
        return grey(0x24)
    return grey(PANEL - speck(x, y, salt) % 3 - (y - 2) * 0.35)


def lit(x, y, cx, cy, radius):
    """-1 (toward the light, top left) to 1 (away from it) across a round part."""
    return ((x - cx) + (y - cy)) / (radius * 1.5)


def cone(x, y, cx, cy, radius):
    """A loudspeaker cone seen from the front, or None outside it: the rim, the rubber
    surround, the hollow cone (its far wall catches the light) and the dust cap."""
    d = math.hypot(x - cx, y - cy)
    if d > radius:
        return None
    if d > radius - 1.0:
        return grey(0x52 - 0x16 * lit(x, y, cx, cy, radius))
    if d > radius - 2.0:
        return grey(0x0d)
    cap = max(1.1, radius * 0.3)
    if d > cap:
        return grey(0x30 + 0x14 * lit(x, y, cx, cy, radius) - 0x08 * (d - cap) / (radius - 2.0 - cap))
    return grey(0x62 - 0x26 * lit(x, y, cx, cy, cap))


def dome(x, y, cx, cy, radius):
    """A tweeter's dome, or None outside it: a dark ring round a small bright cap."""
    d = math.hypot(x - cx, y - cy)
    if d > radius:
        return None
    if d > radius - 1.0:
        return grey(0x0d)
    k = lit(x, y, cx, cy, radius - 1.0)
    # The remote controller's tan (`rctrl.png`), so the dome reads as metal and not as one more cone.
    return rgb(0xdc - 0x40 * k, 0xb4 - 0x38 * k, 0x91 - 0x34 * k)


def tweeter(x, y):
    # The horn: an oval mouth, wider than tall, its walls darker toward the throat where the
    # dome sits; the lower right wall catches the light, as the cones do.
    if math.hypot(x - 7.5, y - 7.5) <= 2.6:
        return dome(x, y, 7.5, 7.5, 2.6)
    dx, dy = x - 7.5, y - 7.5
    if math.hypot(dx / 6.1, dy / 4.4) > 1.0:
        return cabinet(x, y, 2)
    k = (dx / 6.1 + dy / 4.4) / 1.5
    wall = math.hypot(dx / 5.1, dy / 3.4)
    if wall > 1.0:
        return grey(0x52 - 0x16 * k)
    return grey(0x10 + 0x22 * wall + 0x12 * k)


def bass(x, y):
    return cone(x, y, 7.5, 7.5, 6.6) or cabinet(x, y, 1)


def mix(x, y):
    return dome(x, y, 7.5, 3.5, 2.3) or cone(x, y, 7.5, 9.9, 4.7) or cabinet(x, y, 3)


SPEAKER_ART = {"speaker_side": lambda x, y: cabinet(x, y, 0), "speaker_tweeter": tweeter,
               "speaker_bass": bass, "speaker_mix": mix}
for name, art in SPEAKER_ART.items():
    png("assets/%s/textures/block/%s.png" % (MOD, name), 16, 16, art)

# ---- items. The Java ids (ItemRegistry), with their stack sizes. Each copies paper, an
# item with no behaviour of its own: what they do is the native mod's. The keyboard is the
# one that places a block (the left half; the Java KeyboardItem placed both).
TOOLS = ("linker", "screenpen", "ownerthief", "screencfg", "minepad", "laserpointer")
# CraftComponent, without BADEXTCARD; DefaultUpgrade, without REDINPUT, REDOUTPUT and GPS.
COMPONENTS = tuple("craftcomp_" + name for name in (
    "stonekey", "upgrade", "peripheral", "batcell", "batpack", "laserdiode", "backlight", "extcard"))
UPGRADES = ("upgrade_lasermouse",)

items = {"keyboard": {"template": "minecraft:paper", "place_block": ident("kb_left")}}
for name in TOOLS:
    items[name] = {"template": "minecraft:paper", "max_stack_size": 1}
for name in UPGRADES + COMPONENTS:
    items[name] = {"template": "minecraft:paper"}
for name, item in items.items():
    write("data/%s/horizon/item/%s.json" % (MOD, name), item)

# 26.3 items name their model in assets/<ns>/items/. The Java item models are plain ones
# (no overrides): flat sprites, the minePad's small 3D slab, and the blocks' cubes. A
# speaker's item is its block model.
ITEM_MODELS = {name: "item/" + name for name in ("screen", "rctrl", "redctrl") + tuple(items)}
ITEM_MODELS.update((name, "block/" + name) for name in SPEAKERS)
for name, model in ITEM_MODELS.items():
    write("assets/%s/items/%s.json" % (MOD, name), {"model": {"type": "minecraft:model", "model": ident(model)}})

# The Java tab's order (WDTabs), the speakers after the blocks that go with a screen.
TAB = (("screen", "keyboard", "linker", "screenpen", "rctrl", "redctrl") + SPEAKERS
       + ("ownerthief", "screencfg", "minepad", "laserpointer") + UPGRADES + COMPONENTS)
write("data/%s/horizon/creative_tab/main.json" % MOD, {
    # A lang key, as the Java tab's title.
    "title": "itemGroup.%s" % MOD,
    "icon": ident("screen"),
    "items": [ident(name) for name in TAB],
})

# ---- recipes, in 26.3's form: an ingredient is an item id or a #tag, a result an `id`
# and a `count`. The Java ones, file by file, with the 1.12 metadata (`data`) they still
# carried read back: the minePad's `craftcomp_batcell` with data 4 is the battery pack
# (CraftComponent's fifth), which no recipe would use otherwise.
#
# Dropped: `server`, `upgrade_gps`, `upgrade_redin`, `upgrade_redout` (their results are
# gone), `redctrl2` (a redstone controller from a redstone controller) and `minepad2`
# (eight diamond blocks round a minePad for the "minePad 2", which was the minePad's
# metadata 1: since the flattening it gives back the minePad put in).


def shaped(name, category, pattern, key, result, count=1):
    used = set("".join(pattern)) - {" "}
    assert used == set(key), (name, used, set(key))
    result = {"id": ident(result)}
    if count > 1:
        result["count"] = count
    write("data/%s/recipe/%s.json" % (MOD, name), {
        "type": "minecraft:crafting_shaped",
        "category": category,
        "key": dict(sorted(key.items())),
        "pattern": pattern,
        "result": result,
    })


shaped("backlight", "misc", ["PPP", "GGG"],
       {"P": "minecraft:glass_pane", "G": "minecraft:glowstone_dust"}, "craftcomp_backlight")
shaped("batcell", "misc", ["RGR", "RIR", "RGR"],
       {"I": "minecraft:iron_ingot", "G": "minecraft:gold_ingot", "R": "minecraft:redstone"}, "craftcomp_batcell")
shaped("batpack", "misc", ["BBB"], {"B": ident("craftcomp_batcell")}, "craftcomp_batpack")
shaped("extcard", "misc", ["CTR", "IGD"],
       {"C": "minecraft:comparator", "T": "minecraft:redstone_torch", "R": "minecraft:repeater",
        "I": "minecraft:iron_ingot", "G": "minecraft:gold_ingot", "D": "minecraft:redstone"}, "craftcomp_extcard")
shaped("keyboard", "redstone", ["KKK", "KPK", "KKK"],
       {"K": ident("craftcomp_stonekey"), "P": ident("craftcomp_peripheral")}, "keyboard")
shaped("laserdiode", "misc", ["IGI", "IRI", " A "],
       {"I": "minecraft:iron_ingot", "G": "minecraft:glowstone_dust", "R": "minecraft:redstone",
        "A": "minecraft:gold_ingot"}, "craftcomp_laserdiode", 2)
shaped("laserpointer", "equipment", ["D", "C", "B"],
       {"D": ident("craftcomp_laserdiode"), "C": ident("craftcomp_batcell"), "B": "minecraft:oak_button"},
       "laserpointer")
shaped("linker", "equipment", ["R  ", "S  ", "EB "],
       {"R": "minecraft:redstone", "S": "minecraft:stick", "E": "minecraft:ender_pearl",
        "B": "minecraft:stone_button"}, "linker")
shaped("minepad", "equipment", ["PPP", "RGB", "DCD"],
       {"P": "minecraft:glass_pane", "R": "minecraft:red_dye", "G": "minecraft:green_dye",
        "B": "minecraft:blue_dye", "D": "minecraft:diamond", "C": ident("craftcomp_batpack")}, "minepad")
shaped("peripheral", "misc", ["SSS", "STS", "SSS"],
       {"S": "minecraft:oak_slab", "T": "minecraft:redstone_torch"}, "craftcomp_peripheral")
shaped("rctrl", "redstone", [" P ", "BBB"],
       {"P": ident("craftcomp_peripheral"), "B": "minecraft:oak_button"}, "rctrl")
# `redctrl1` in Java, when there was a second.
shaped("redctrl", "redstone", ["RRR", "RPR", "BBB"],
       {"R": "minecraft:redstone", "P": ident("craftcomp_peripheral"), "B": "minecraft:oak_button"}, "redctrl")
shaped("screen", "redstone", ["PPP", "RGB", "LLL"],
       {"P": "minecraft:glass_pane", "R": "minecraft:red_dye", "G": "minecraft:green_dye",
        "B": "minecraft:blue_dye", "L": ident("craftcomp_backlight")}, "screen", 8)
shaped("screencfg", "equipment", ["T T", "PDP", "PPP"],
       {"P": "minecraft:glass_pane", "T": "minecraft:redstone_torch", "D": "minecraft:diamond"}, "screencfg")
shaped("stonekey", "misc", ["P"], {"P": "minecraft:stone_pressure_plate"}, "craftcomp_stonekey", 9)
shaped("upgrade", "misc", [" Q ", "QEQ", " Q "],
       {"Q": "minecraft:quartz", "E": ident("craftcomp_extcard")}, "craftcomp_upgrade")
shaped("upgrade_laser", "misc", ["UL"],
       {"U": ident("craftcomp_upgrade"), "L": ident("craftcomp_laserdiode")}, "upgrade_lasermouse")

# New. The screen pen had no recipe in Java (creative only): a stick with a glowing tip
# and the linker's button. The speakers are a note block in an iron cabinet, wired with
# redstone: a quartz dome for the tweeter (the cheapest), a wool cone for the bass, and
# both with a comparator between them, to split the signal, for the full range one (the
# dearest).
shaped("screenpen", "equipment", ["  G", " S ", "B  "],
       {"G": "minecraft:glowstone_dust", "S": "minecraft:stick", "B": "minecraft:stone_button"}, "screenpen")
shaped("speaker_tweeter", "redstone", [" Q ", "INI", " R "],
       {"Q": "minecraft:quartz", "I": "minecraft:iron_ingot", "N": "minecraft:note_block",
        "R": "minecraft:redstone"}, "speaker_tweeter")
shaped("speaker_bass", "redstone", ["IWI", "WNW", "IRI"],
       {"I": "minecraft:iron_ingot", "W": "#minecraft:wool", "N": "minecraft:note_block",
        "R": "minecraft:redstone"}, "speaker_bass")
shaped("speaker_mix", "redstone", ["IQI", "WNW", "ICI"],
       {"I": "minecraft:iron_ingot", "Q": "minecraft:quartz", "W": "#minecraft:wool",
        "N": "minecraft:note_block", "C": "minecraft:comparator"}, "speaker_mix")

# ---- advancements: the Java tree, in 26.3's form (the folder is `advancement`, an icon
# is an `id`, a background names its texture without `textures/` and `.png`). Four of
# them are granted by triggers of the mod's own, which have no conditions: the native mod
# fires `webdisplays:<trigger>` (Host::trigger_criterion) where the Java mod called
# Criterion.trigger. The recipes each one unlocks are the Java ones still here, the new
# ones with the first screen.


def advancement(name, icon, text, criterion, trigger, conditions=None, parent=None, recipes=(), quiet=False):
    display = {
        "icon": {"id": icon},
        "title": {"translate": "advancements.%s.%s.title" % (MOD, text)},
        "description": {"translate": "advancements.%s.%s.description" % (MOD, text)},
        "frame": "task",
    }
    if parent is None:
        display["background"] = "minecraft:gui/advancements/backgrounds/stone"
    if quiet:
        display["show_toast"] = False
        display["announce_to_chat"] = False
    value = {"display": display}
    if parent:
        value["parent"] = ident(parent)
    value["criteria"] = {criterion: {"trigger": trigger}}
    if conditions:
        value["criteria"][criterion]["conditions"] = conditions
    value["requirements"] = [[criterion]]
    if recipes:
        value["rewards"] = {"recipes": [ident(recipe) for recipe in recipes]}
    write("data/%s/advancement/%s.json" % (MOD, name), value)


def holding(item):
    return {"items": [{"items": item}]}


advancement("root", "minecraft:glass_pane", "root", "has_glass_pane", "minecraft:inventory_changed",
            holding("minecraft:glass_pane"), quiet=True,
            recipes=("backlight", "screen", "batcell", "batpack", "minepad"))
advancement("screen", ident("screen"), "screen", "has_screen", "minecraft:inventory_changed",
            holding(ident("screen")), parent="root",
            recipes=("linker", "screencfg", "peripheral", "rctrl", "redctrl", "stonekey", "keyboard",
                     "screenpen") + SPEAKERS)
# Linking a peripheral to a screen with the linker (ItemLinker.useOn, once connect() succeeds).
advancement("link_peripheral", ident("linker"), "linkperipheral", "link_peripheral",
            ident("link_peripheral"), parent="screen")
# An ocelot walking on a linked keyboard types a letter: granted to the screen's owner
# (KeyboardBlockEntity.simulateCat).
advancement("keyboard_cat", "minecraft:string", "keyboardcat", "keyboard_cat", ident("keyboard_cat"),
            parent="link_peripheral")
# A minePad thrown from 20 blocks or higher breaks on landing and leaves an extension card:
# granted to the thrower (ItemMinePad2.onEntityItemUpdate).
advancement("pad_break", ident("minepad"), "padbreak", "pad_break", ident("pad_break"), parent="root",
            recipes=("extcard", "upgrade", "laserdiode", "laserpointer", "upgrade_laser"))
# The first upgrade installed in a screen (ScreenBlock.use, once addUpgrade() succeeds).
advancement("upgrade", ident("screen"), "upgrade", "upgrade_screen", ident("upgrade_screen"),
            parent="pad_break")

# ---- sounds: the Java events and their .ogg files (copied above), without the file
# server's. `category` is not a field of sounds.json any more: whoever plays a sound says
# where it is heard (the Java mod: keyboard_type, upgrade_add and upgrade_del as blocks,
# screencfg_open as master, ironic as players).
sounds = read("assets/%s/sounds.json" % MOD)
del sounds["server"]
write("assets/%s/sounds.json" % MOD, {event: {"sounds": entry["sounds"]} for event, entry in sounds.items()})

# ---- languages. The Java files, without the keys of what was dropped and of what 1.12
# left behind (the metadata items' base names, the minePad 2), with the names 26.3 looks
# up for the blocks and items that are new or have another id here, French brought to the
# English key set, and the native mod's own sentences in English and French (LANG_NATIVE).
# A language that lacks a key shows the English text.
LANG_DROPPED = re.compile(
    r"block\.webdisplays\.(ccinterface|cointerface|server|peripheral)$"
    r"|item\.webdisplays\.(craftcomp|craftcomp_badextcard|upgrade|upgrade_redinput|upgrade_redoutput|upgrade_gps"
    r"|minepad2|advicon(\..*)?)$"
    r"|webdisplays\.server\.|webdisplays\.extcard\.|webdisplays\.minepad2\."
    r"|webdisplays\.message\.(missingCC|missingOC|noMiniserv)$"
    r"|advancements\.webdisplays\.minepad2\.")
# A new key takes the text of the Java key it stands for.
LANG_SAME = {
    "block.webdisplays.screen_core": "block.webdisplays.screen",
    "block.webdisplays.kb_right": "block.webdisplays.kb_left",
    "item.webdisplays.keyboard": "block.webdisplays.kb_left",
    "block.webdisplays.polyscreen_core": "block.webdisplays.polyscreen",
    # What translates the tab's title whatever the tab file says.
    "itemGroup.webdisplays.main": "itemGroup.webdisplays",
}
LANG_NEW = {
    "en_us": {
        "block.webdisplays.speaker_tweeter": "Tweeter Speaker",
        "block.webdisplays.speaker_bass": "Bass Speaker",
        "block.webdisplays.speaker_mix": "Full-Range Speaker",
        # A slip of the Java file.
        "webdisplays.message.chunkUnloaded": "The chunk the screen is placed in is not loaded!",
    },
    "fr_fr": {
        "block.webdisplays.speaker_tweeter": "Enceinte d'aigus",
        "block.webdisplays.speaker_bass": "Enceinte de basses",
        "block.webdisplays.speaker_mix": "Enceinte large bande",
        # English keys the Java French file never had.
        "webdisplays.gui.screencfg.volume": "Volume :",
        "webdisplays.gui.screencfg.setvol": "Régler le volume",
        "webdisplays.key.toggle_mouse": "Activer/désactiver la souris laser (WebDisplays)",
        "webdisplays.gui.minepad.close": "Appuyez sur Maj et Échap pour fermer.",
        # What the Java French file has without its accents, or left in English.
        "block.webdisplays.polyscreen": "Écran tracé",
        "block.webdisplays.polyscreen_core": "Écran tracé",
        "item.webdisplays.screenpen": "Stylet d'écran",
        "webdisplays.gui.seturl.browsers": "Navigateurs partagés",
        "webdisplays.gui.seturl.link.unlink": "Délier",
    },
    "zh_cn": {
        "block.webdisplays.speaker_tweeter": "高音扬声器",
        "block.webdisplays.speaker_bass": "低音扬声器",
        "block.webdisplays.speaker_mix": "全频扬声器",
    },
}
# The native mod's own sentences (native/api/src): key: (English, French). The Java mod has
# none of them; Chinese shows the English until someone translates them. The English text
# is the one the Rust code carries for a client without these files (`Phrase::new(key,
# english)`): check_content.py reads the Rust sources and refuses a key that is missing
# here, or whose English differs. Horizon fills `%s` (and `%1$s`) only, and `%%` is a
# percent sign. Labels of the windows sit in fixed boxes: check_content.py knows how many
# characters each may be (FITS).
LANG_NATIVE = {
    # ---- what more than one window or message says
    "webdisplays.h.gui.name": ("Name", "Nom"),
    "webdisplays.h.gui.pixels": ("Pixels", "Pixels"),
    "webdisplays.h.gui.close": ("Close", "Fermer"),
    "webdisplays.h.gui.save": ("Save", "Enregistrer"),
    "webdisplays.h.address.malformed": ("That is not an address a browser can open.", "Cette adresse ne peut pas être ouverte."),
    "webdisplays.h.address.too_long": ("That address is too long.", "Cette adresse est trop longue."),
    "webdisplays.h.description": ("Web browsers on screens in the world.", "Des navigateurs web sur des écrans, dans le monde."),
    # ---- screens: made, looked at, given an address
    "webdisplays.h.screen.home": ("Right-click to choose a page", "Clic droit pour choisir une page"),
    "webdisplays.h.screen.too_small": ("A screen needs at least two blocks.", "Un écran a besoin d'au moins deux blocs."),
    "webdisplays.h.screen.too_big": ("A screen can be %sx%s blocks at most.", "Un écran mesure au plus %sx%s blocs."),
    "webdisplays.h.screen.look": ("Look at a screen.", "Regardez un écran."),
    "webdisplays.h.screen.lost_data": ("That screen has lost its data.", "Cet écran a perdu ses données."),
    "webdisplays.h.screen.not_yet": ("That is not a screen yet: right-click it with an empty hand.",
                                     "Ce n'est pas encore un écran : faites un clic droit dessus, la main vide."),
    "webdisplays.h.screen.not_allowed": ("The owner of that screen does not let you do this.",
                                         "Le propriétaire de cet écran ne vous y autorise pas."),
    "webdisplays.h.screen.url_not_allowed": ("You may not change what this screen shows.",
                                             "Vous ne pouvez pas changer ce qu'affiche cet écran."),
    "webdisplays.h.screen.shows": ("The screen now shows %s", "L'écran affiche maintenant %s"),
    "webdisplays.h.command.which_address": ("Which address?", "Quelle adresse ?"),
    "webdisplays.h.command.usage": ("Usage: /webdisplays url <address>, /webdisplays curve, /webdisplays flip",
                                    "Utilisation : /webdisplays url <adresse>, /webdisplays curve, /webdisplays flip"),
    # ---- the linking tool and what is linked
    "webdisplays.h.linker.pick_first": ("Click a screen with the linking tool first.",
                                        "Cliquez d'abord sur un écran avec l'outil d'appairage."),
    "webdisplays.h.linker.picked": ("Screen picked. Now click each keyboard, remote, controller or speaker to link to it.",
                                    "Écran sélectionné. Cliquez maintenant sur chaque clavier, télécommande, contrôleur ou enceinte"
                                    " à lui appairer."),
    "webdisplays.h.linker.gone": ("The screen you picked is gone.", "L'écran sélectionné n'existe plus."),
    "webdisplays.h.linker.linked": ("Linked to the screen. It stays picked: click the next thing to link to it.",
                                    "Appairé à l'écran. Il reste sélectionné : cliquez sur le prochain périphérique à appairer."),
    "webdisplays.h.linker.cannot": ("This cannot be linked.", "Impossible d'appairer ceci."),
    "webdisplays.h.linker.speakers_full": ("A screen plays through %s speakers at most.", "Un écran utilise %s enceintes au maximum."),
    "webdisplays.h.linker.not_linked": ("Not linked to a screen: click a screen with the linking tool, then this.",
                                        "Pas appairé à un écran : cliquez sur un écran avec l'outil d'appairage, puis sur ceci."),
    "webdisplays.h.linker.screen_gone": ("The screen it was linked to is gone, or too far away to answer.",
                                         "L'écran auquel il était appairé n'existe plus, ou est trop loin pour répondre."),
    "webdisplays.h.keyboard.half_lost": ("This half of a keyboard has lost its other half.",
                                         "Cette moitié de clavier a perdu son autre moitié."),
    "webdisplays.h.keyboard.no_room": ("A keyboard needs a free block to its right.", "Un clavier a besoin d'un bloc libre à sa droite."),
    "webdisplays.h.keyboard.typing": ("Typing on the screen. Escape to stop.", "Saisie sur l'écran. Échap pour arrêter."),
    "webdisplays.h.speaker.both": ("both channels", "les deux canaux"),
    "webdisplays.h.speaker.left": ("left channel", "canal gauche"),
    "webdisplays.h.speaker.right": ("right channel", "canal droit"),
    "webdisplays.h.speaker.tuned": ("Speaker: %s, %s%% (click: channel, sneak-click: volume)",
                                    "Enceinte : %s, %s %% (clic : canal, clic accroupi : volume)"),
    "webdisplays.h.speaker.not_allowed": ("The owner of its screen does not let you change this speaker.",
                                          "Le propriétaire de l'écran ne vous laisse pas régler cette enceinte."),
    "webdisplays.h.speaker.screen_gone": ("The screen it was linked to is gone: link it again.",
                                          "L'écran auquel elle était appairée n'existe plus : appairez-la de nouveau."),
    # ---- the owner thief and the upgrades
    "webdisplays.h.thief.creative_only": ("Only in creative mode.", "En mode créatif uniquement."),
    "webdisplays.h.thief.yours": ("The screen is yours.", "L'écran est à vous."),
    "webdisplays.h.upgrade.not_allowed": ("You may not change this screen's upgrades.",
                                          "Vous ne pouvez pas modifier les améliorations de cet écran."),
    "webdisplays.h.upgrade.already": ("This screen has that upgrade already.", "Cet écran a déjà cette amélioration."),
    "webdisplays.h.upgrade.laser_installed": ("Laser upgrade installed: a laser pointer now works this screen from afar.",
                                              "Capteur de laser installé : un pointeur laser commande maintenant cet écran à distance."),
    # ---- joining two screens (the list of the world's screens)
    "webdisplays.h.join.too_far": ("That screen is too far away to be joined now: go nearer to it once, or load its chunk.",
                                   "Cet écran est trop loin pour être joint maintenant : approchez-vous-en une fois, ou chargez son"
                                   " chunk."),
    "webdisplays.h.join.not_allowed": ("You must be allowed to change both screens to join them.",
                                       "Vous devez avoir le droit de modifier les deux écrans pour les joindre."),
    "webdisplays.h.join.done": ("Joined: the two screens now show one picture between them.",
                                "Joints : les deux écrans affichent maintenant une seule image à eux deux."),
    "webdisplays.h.join.facing": ("Joined, but they face different ways, so they cannot share one picture: give them a shared browser"
                                  " instead.",
                                  "Joints, mais ils ne sont pas orientés de la même façon et ne peuvent pas partager une image :"
                                  " donnez-leur plutôt un navigateur partagé."),
    # ---- traced screens (the pen, /webdisplays curve and flip)
    "webdisplays.h.poly.corners": ("%s corner(s). Click the first one to close; click a corner again to remove it; sneak-click"
                                   " wipes all.",
                                   "%s coin(s). Clic sur le premier : fermer ; sur un coin déjà posé : le retirer ;"
                                   " clic accroupi : tout effacer."),
    "webdisplays.h.poly.corner_removed": ("Corner removed: %s left.", "Coin retiré : il en reste %s."),
    "webdisplays.h.poly.full": ("That outline has as many corners as it can: click its first corner to close it.",
                                "Ce tracé ne peut pas avoir plus de coins : cliquez sur son premier coin pour le fermer."),
    "webdisplays.h.poly.cannot_add": ("That corner cannot be added.", "Impossible d'ajouter ce coin."),
    "webdisplays.h.poly.refused": ("That outline cannot be a screen (%s).", "Ce tracé ne peut pas devenir un écran (%s)."),
    "webdisplays.h.poly.reason.degenerate": ("degenerate", "sans surface"),
    "webdisplays.h.poly.reason.too_wide": ("too wide", "trop large"),
    "webdisplays.h.poly.reason.too_tall": ("too tall", "trop haut"),
    "webdisplays.h.poly.reason.too_many_cells": ("too many cells", "trop de cellules"),
    "webdisplays.h.poly.reason.obstructed": ("obstructed", "obstrué"),
    "webdisplays.h.poly.blocks_missing": ("The screen's blocks are missing from the game.", "Les blocs de l'écran sont absents du jeu."),
    "webdisplays.h.poly.cannot_build": ("The screen could not be built here.", "L'écran n'a pas pu être construit ici."),
    "webdisplays.h.poly.traced": ("Screen traced: %sx%s blocks. Right-click it to choose a page.",
                                  "Écran tracé : %sx%s blocs. Clic droit dessus pour choisir une page."),
    "webdisplays.h.poly.look": ("Look at a traced screen.", "Regardez un écran tracé."),
    "webdisplays.h.poly.not_allowed": ("The owner of this screen does not let you change it.",
                                       "Le propriétaire de cet écran ne vous laisse pas le modifier."),
    "webdisplays.h.poly.lost_outline": ("That screen has lost its outline.", "Cet écran a perdu son tracé."),
    "webdisplays.h.poly.no_surface": ("That outline would be no surface.", "Ce tracé ne délimiterait aucune surface."),
    "webdisplays.h.poly.rounded": ("The screen's corners are now rounded.", "Les coins de l'écran sont maintenant arrondis."),
    "webdisplays.h.poly.sharp": ("The screen's corners are now sharp.", "Les coins de l'écran sont maintenant vifs."),
    "webdisplays.h.poly.flipped": ("The screen now faces the other way.", "L'écran fait maintenant face de l'autre côté."),
    "webdisplays.h.poly.radius": ("The screen's corners now have a radius of %s blocks.", "Les coins de l'écran ont maintenant un rayon de %s blocs."),
    # ---- the address window (client/gui/url.rs) and the minePad's bar
    "webdisplays.h.url.title": ("Screen address", "Adresse de l'écran"),
    "webdisplays.h.url.hint": ("Enter opens it, Escape closes.", "Entrée pour ouvrir, Échap pour fermer."),
    "webdisplays.h.url.open": ("Open", "Ouvrir"),
    "webdisplays.h.url.empty": ("Type an address.", "Saisissez une adresse."),
    "webdisplays.h.pad.go": ("Go", "Aller"),
    # ---- the redstone controller (client/gui/redstone.rs)
    "webdisplays.h.redstone.title": ("Redstone controller", "Contrôleur redstone"),
    "webdisplays.h.redstone.rising": ("When the signal comes on, show", "Quand le signal arrive, afficher"),
    "webdisplays.h.redstone.falling": ("When it goes off, show", "Quand il s'arrête, afficher"),
    "webdisplays.h.redstone.refused": ("One of these is not an address a browser can open.",
                                       "Une de ces adresses ne peut pas être ouverte."),
    "webdisplays.h.redstone.set": ("Redstone controller set.", "Contrôleur redstone réglé."),
    # ---- the screen's configuration window (client/gui/config.rs)
    "webdisplays.h.config.title": ("Screen %sx%s, owned by %s", "Écran %sx%s, propriétaire : %s"),
    "webdisplays.h.config.nobody": ("nobody", "personne"),
    "webdisplays.h.config.set": ("Set", "OK"),
    "webdisplays.h.config.best_fit": ("Best fit", "Taille idéale"),
    "webdisplays.h.config.turned": ("Turned %s°", "Rotation %s°"),
    "webdisplays.h.config.edges.blocks": ("Edges: blocks", "Bords : blocs"),
    "webdisplays.h.config.edges.cut": ("Edges: cut corners", "Bords : coins coupés"),
    "webdisplays.h.config.edges.slanted": ("Edges: slanted", "Bords : en biais"),
    "webdisplays.h.config.rounded": ("Rounded corners", "Coins arrondis"),
    "webdisplays.h.config.flip": ("Turn around", "Retourner"),
    "webdisplays.h.config.oled": ("Black is see-through", "Le noir est transparent"),
    "webdisplays.h.config.auto_volume": ("Quieter with distance", "Moins fort avec la distance"),
    "webdisplays.h.config.volume": ("Volume", "Volume"),
    "webdisplays.h.config.percent": ("%s%%", "%s %%"),
    "webdisplays.h.config.link": ("Link", "Lien"),
    "webdisplays.h.config.link_mode.joined": ("Linked screens: as if touching", "Écrans liés : bord à bord"),
    "webdisplays.h.config.link_mode.spaced": ("Linked screens: gaps hide picture", "Écrans liés : écart compris"),
    "webdisplays.h.config.shared": ("Shared...", "Partage..."),
    "webdisplays.h.config.shared_on": ("Shared: on...", "Partage : oui"),
    "webdisplays.h.config.all_screens": ("All screens...", "Les écrans..."),
    "webdisplays.h.config.may": ("May:", "Droits :"),
    "webdisplays.h.config.friends": ("Friends", "Amis"),
    "webdisplays.h.config.right.url": ("Change the address", "Changer l'adresse"),
    "webdisplays.h.config.right.interact": ("Click and type", "Cliquer et écrire"),
    "webdisplays.h.config.right.settings": ("Change settings", "Régler l'écran"),
    "webdisplays.h.config.right.upgrades": ("Manage upgrades", "Améliorer l'écran"),
    "webdisplays.h.config.right.others": ("Set others' rights", "Droits des autres"),
    "webdisplays.h.config.add": ("Add", "OK"),
    "webdisplays.h.config.remove": ("Remove", "Retirer"),
    "webdisplays.h.config.more_friends": ("and %s more", "et %s de plus"),
    "webdisplays.h.config.upgrade.laser": ("Laser pointer upgrade", "Capteur de laser"),
    "webdisplays.h.config.take_out": ("Take out", "Retirer"),
    "webdisplays.h.config.refused.not_allowed": ("The owner of this screen does not let you change that.",
                                                 "Le propriétaire de cet écran ne vous laisse pas modifier cela."),
    "webdisplays.h.config.refused.nobody": ("Nobody by that name is on the server.", "Aucun joueur de ce nom n'est sur le serveur."),
    "webdisplays.h.config.refused.friend": ("They are on the list already, or the list is full.",
                                            "Ce joueur est déjà dans la liste, ou la liste est pleine."),
    # ---- the shared browsers (client/gui/browsers.rs, server/browsers.rs)
    "webdisplays.h.browsers.title": ("Shared browsers: one page on as many screens as you like",
                                     "Navigateurs partagés : une page sur autant d'écrans que voulu"),
    "webdisplays.h.browsers.none": ("None yet. Make one, then choose it on each screen that should show it.",
                                    "Aucun. Créez-en un, puis choisissez-le sur chaque écran voulu."),
    "webdisplays.h.browsers.show": ("Show", "Afficher"),
    "webdisplays.h.browsers.shown": ("Shown", "Affiché"),
    "webdisplays.h.browsers.change": ("Change", "Modifier"),
    "webdisplays.h.browsers.own": ("Own page", "Page propre"),
    "webdisplays.h.browsers.more": ("More", "Suite"),
    "webdisplays.h.browsers.back": ("Back", "Retour"),
    "webdisplays.h.browsers.form.new": ("New shared browser", "Nouveau navigateur partagé"),
    "webdisplays.h.browsers.form.change": ("Change a shared browser", "Modifier un navigateur partagé"),
    "webdisplays.h.browsers.form.address": ("Address", "Adresse"),
    "webdisplays.h.browsers.bad_size": ("The size is two numbers of pixels.", "La taille s'écrit en deux nombres de pixels."),
    "webdisplays.h.browsers.full": ("This world has as many shared browsers as it can.",
                                    "Ce monde ne peut pas avoir plus de navigateurs partagés."),
    "webdisplays.h.browsers.unknown": ("That shared browser no longer exists.", "Ce navigateur partagé n'existe plus."),
    "webdisplays.h.browsers.not_yours": ("Only whoever made that shared browser, or an operator, may change it.",
                                         "Seul le créateur de ce navigateur partagé, ou un opérateur, peut le modifier."),
    # ---- the list of the world's screens (client/gui/screens.rs)
    "webdisplays.h.screens.title": ("Screens of this world", "Écrans de ce monde"),
    "webdisplays.h.screens.none": ("None listed yet.", "Aucun pour l'instant."),
    "webdisplays.h.screens.no_name": ("(no name)", "(sans nom)"),
    "webdisplays.h.screens.traced": ("traced", "tracé"),
    "webdisplays.h.screens.side.floor": ("floor", "sol"),
    "webdisplays.h.screens.side.ceiling": ("ceiling", "plafond"),
    "webdisplays.h.screens.side.north": ("north", "nord"),
    "webdisplays.h.screens.side.south": ("south", "sud"),
    "webdisplays.h.screens.side.west": ("west", "ouest"),
    "webdisplays.h.screens.side.east": ("east", "est"),
    "webdisplays.h.screens.this": ("This", "Celui-ci"),
    "webdisplays.h.screens.join": ("Join", "Joindre"),
    "webdisplays.h.screens.previous": ("Previous", "Précédent"),
    "webdisplays.h.screens.next": ("Next", "Suivant"),
    # ---- the settings page (client/settings.rs)
    "webdisplays.h.settings.volume": ("Volume of screens: %s%%", "Volume des écrans : %s %%"),
    "webdisplays.h.settings.echo": ("Echo of the place: %s", "Écho du lieu : %s"),
    "webdisplays.h.settings.glow": ("Light thrown by screens: %s", "Éclairage des écrans : %s"),
    "webdisplays.h.settings.sound": ("Sound for: %s", "Son pour : %s"),
    # In French the two settings above are masculine, so that one word fits both.
    "webdisplays.h.settings.level.off": ("off", "désactivé"),
    "webdisplays.h.settings.level.soft": ("discreet", "discret"),
    "webdisplays.h.settings.level.normal": ("normal", "normal"),
    "webdisplays.h.settings.level.strong": ("strong", "fort"),
    "webdisplays.h.settings.headphones": ("headphones", "casque"),
    "webdisplays.h.settings.loudspeakers": ("loudspeakers", "haut-parleurs"),
    # ---- the browser: why it is down (client/world.rs), then HCEF's own sentences, which this
    # mod translates since it shows them (HCEF/native/api/src: ipc/host.rs, install.rs)
    "webdisplays.h.host.failed": ("WebDisplays: %s", "WebDisplays : %s"),
    "hcef.host.not_installed": ("hcef-host is not installed (%s)", "hcef-host n'est pas installé (%s)"),
    "hcef.host.cannot_listen": ("cannot listen for hcef-host: %s", "impossible d'écouter hcef-host : %s"),
    "hcef.host.cannot_start": ("cannot start hcef-host: %s", "impossible de démarrer hcef-host : %s"),
    "hcef.host.stopped_starting": ("hcef-host stopped as it started (%s)", "hcef-host s'est arrêté au démarrage (%s)"),
    "hcef.host.no_connect": ("hcef-host did not connect", "hcef-host ne s'est pas connecté"),
    "hcef.host.silent": ("hcef-host connected but said nothing", "hcef-host s'est connecté mais n'a rien dit"),
    "hcef.host.version": ("hcef-host speaks version %s, the mod %s: update one of them",
                                   "hcef-host parle la version %s, le mod la %s : mettez l'un des deux à jour"),
    "hcef.host.impostor": ("a program that is not hcef-host answered", "un programme qui n'est pas hcef-host a répondu"),
    "hcef.host.hung_up": ("hcef-host hung up as it started (%s)", "hcef-host a raccroché au démarrage (%s)"),
    "hcef.host.nonsense": ("hcef-host sent a message that makes no sense (%s)",
                                    "hcef-host a envoyé un message incompréhensible (%s)"),
    "hcef.host.stopped": ("hcef-host stopped", "hcef-host s'est arrêté"),
    "hcef.host.link_broke": ("the link to hcef-host broke (%s)", "la liaison avec hcef-host est rompue (%s)"),
    "hcef.description": ("A Chromium browser for other mods.", "Un navigateur Chromium pour les autres mods."),
    "hcef.install.button.downloading": ("Browser: downloading, %s%%", "Navigateur : téléchargement, %s %%"),
    "hcef.install.button.unpacking": ("Browser: unpacking...", "Navigateur : décompression..."),
    "hcef.install.button.failed": ("Browser: download failed (%s). Click to try again",
                                   "Navigateur : échec du téléchargement (%s). Cliquez pour réessayer"),
    "hcef.install.button.installed": ("Browser: installed (build %s)", "Navigateur : installé (version %s)"),
    "hcef.install.button.no_release": ("Browser: this build of the mod has no download; install hcef-host by hand",
                                       "Navigateur : cette version du mod n'a pas de téléchargement ; installez hcef-host à la main"),
    "hcef.install.button.update": ("Browser: another build is installed. Click to download build %s (%s MB)",
                                   "Navigateur : une autre version est installée. Cliquez pour télécharger la version %s (%s Mo)"),
    "hcef.install.button.download": ("Browser: not installed. Click to download it (%s MB, from GitHub)",
                                     "Navigateur : non installé. Cliquez pour le télécharger (%s Mo, depuis GitHub)"),
    "hcef.install.downloading": ("HCEF: downloading the browser, %s%%", "HCEF : téléchargement du navigateur, %s %%"),
    "hcef.install.failed": ("HCEF: the download failed (%s). Options > Mods > HCEF to try again",
                            "HCEF : le téléchargement a échoué (%s). Options > Mods > HCEF pour réessayer"),
    "hcef.install.done": ("HCEF: the browser is installed", "HCEF : le navigateur est installé"),
    "hcef.install.missing": ("HCEF: the browser is not installed. Options > Mods > HCEF downloads it",
                             "HCEF : le navigateur n'est pas installé. Options > Mods > HCEF le télécharge"),
}


def language(code):
    text = {key: value for key, value in read("assets/%s/lang/%s.json" % (MOD, code)).items()
            if not LANG_DROPPED.match(key)}
    for key, source in LANG_SAME.items():
        if source in text:
            text[key] = text[source]
    text.update(LANG_NEW[code])
    taken = sorted(set(text) & set(LANG_NATIVE))
    if taken:
        sys.exit("%s.json: LANG_NATIVE names keys the Java mod has already: %s" % (code, taken))
    if code in ("en_us", "fr_fr"):
        text.update({key: pair[code == "fr_fr"] for key, pair in LANG_NATIVE.items()})
    # The Java title starts with a colour code (purple); a tab's title here is plain text.
    for key in ("itemGroup.webdisplays", "itemGroup.webdisplays.main"):
        if key in text:
            text[key] = text[key].replace("§5", "")
    return text


english = language("en_us")
write("assets/%s/lang/en_us.json" % MOD, english)
for code in ("fr_fr", "zh_cn"):
    text = language(code)
    extra = sorted(set(text) - set(english))
    missing = [key for key in english if key not in text]
    if extra or (missing and code == "fr_fr"):
        sys.exit("%s.json: keys English has not: %s; keys to translate in LANG_NEW: %s" % (code, extra, missing))
    # In the order of the English file.
    write("assets/%s/lang/%s.json" % (MOD, code), {key: text[key] for key in english if key in text})

print("blocks: %d, items: %d, block state parts: %d, assets left behind: %d"
      % (len(blocks), len(items), len(multipart), len(skipped)))
