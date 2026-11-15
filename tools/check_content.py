"""Checks the content mod mkcontent.py wrote, as far as files can be checked without the game.

usage: python -I check_content.py [content mod folder, default ../content] [--vanilla <prepared 26.3 root>]

Exits 1 and lists the problems when there are any. What is checked:
- every path is one a content mod may hold, every JSON file parses;
- block, item and creative tab declarations hold only the fields Horizon's loader reads
  (crates/horizon-mods/src/content.rs refuses any other), with values it accepts;
- every block has a block state, every block state and item names models that exist, every
  model's parents and textures exist, and no texture reference is left unresolved;
- recipes, loot tables, tags and advancements are in 26.3's shape and name items, blocks,
  recipes and advancements that exist;
- sounds.json and the .ogg files match; English and French have the same keys, with a name
  for every block and item, and the Java GUI layouts' labels are keys they have;
- every sentence the native mod's Rust sources name (../native/api/src) is in English and
  in French, with the English text the code carries, placeholders the game fills, and,
  for a label that sits in a box of a window, a translation short enough for the box;
- nothing mentions what was dropped from the Java mod.

Vanilla's own names (templates and their state properties, items, tags, parent models) are
the few listed below. With a prepared 26.3 resource root (the folder holding facts.json and
items.json that the game keeps under .local/horizon-client/26.3; found under the launcher's
versions when not given), those lists are checked against the game's own tables too.
"""
import glob
import json
import os
import re
import struct
import sys

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
args = sys.argv[1:]
vanilla = None
if "--vanilla" in args:
    at = args.index("--vanilla")
    vanilla = args[at + 1]
    del args[at:at + 2]
root = args[0] if args else os.path.join(HERE, "content")
MOD = "webdisplays"

# ---- what this mod uses of vanilla. Each block template with its state properties.
BOOL = ["false", "true"]
SIX = ["down", "east", "north", "south", "up", "west"]
TEMPLATES = {
    "minecraft:chorus_plant": {side: BOOL for side in SIX},
    "minecraft:white_glazed_terracotta": {"facing": ["east", "north", "south", "west"]},
    "minecraft:stone": {},
    "minecraft:barrier": {"waterlogged": BOOL},
    "minecraft:dispenser": {"facing": SIX, "triggered": BOOL},
}
ITEM_TEMPLATES = {"minecraft:paper"}
COMPONENT_TYPES = {"minecraft:container"}
VANILLA_ITEMS = {"minecraft:" + name for name in (
    "blue_dye comparator diamond ender_pearl glass_pane glowstone_dust gold_ingot green_dye iron_ingot "
    "note_block oak_button oak_slab quartz red_dye redstone redstone_torch repeater stick stone_button "
    "stone_pressure_plate string").split()}
VANILLA_ITEM_TAGS = {"minecraft:wool"}
VANILLA_MODELS = {
    # Each parent model with the texture slots it leaves to its children.
    "minecraft:block/cube_all": {"all"},
    "minecraft:block/orientable": {"front", "side", "top"},
    "minecraft:item/generated": {"layer0"},
    "minecraft:item/handheld": {"layer0"},
}
VANILLA_TEXTURES = set()
VANILLA_BACKGROUNDS = {"minecraft:gui/advancements/backgrounds/stone"}
RECIPE_CATEGORIES = {"building", "redstone", "equipment", "misc"}

# The Java mod's parts that were dropped. In a path or in a file's text; of a language
# file only the keys are searched (a message may well say "this server").
DROPPED = re.compile(
    r"ccinterface|ocinterface|cointerface|cci_|oci_|upgrade_gps|upgrade_red(in|out)|badextcard|advicon"
    r"|redctrl2|minepad2|miniserv|webdisplays[:.]server\b|(block|item|blocks|sounds|gui)/server"
    r"|\bmissing(CC|OC)\b", re.I)

problems = []


def problem(where, what):
    problems.append("%s: %s" % (where, what))


def full(name, default="minecraft"):
    return name if ":" in name else "%s:%s" % (default, name)


# ---- the files
files = {}
for top in ("assets", "data"):
    for folder, _, names in os.walk(os.path.join(root, top)):
        for name in names:
            path = os.path.join(folder, name)
            files[os.path.relpath(path, root).replace(os.sep, "/")] = path
if not files:
    sys.exit("%s holds no assets/ or data/" % root)

docs = {}
for rel, path in sorted(files.items()):
    parts = rel.split("/")
    # crates/horizon-mods/src/content.rs, check_path and the namespace fence.
    if len(parts) < 3 or any(not re.fullmatch(r"[a-z0-9_.-]+", p) or p in (".", "..") for p in parts):
        problem(rel, "not a plain assets/ or data/ path")
    top, namespace, rest = parts[0], parts[1], "/".join(parts[2:])
    if not (namespace == MOD or (top == "data" and namespace in ("minecraft", "c") and rest.startswith("tags/"))
            or (top == "assets" and namespace == "minecraft" and rest.startswith("atlases/"))):
        problem(rel, "outside the mod's namespace, vanilla's tags and atlases")
    if os.path.getsize(path) > 16 << 20:
        problem(rel, "over 16 MiB")
    if DROPPED.search(rel):
        problem(rel, "the path names something that was dropped")
    if rel.endswith((".json", ".mcmeta")):
        try:
            with open(path, encoding="utf-8") as f:
                text = f.read()
            docs[rel] = json.loads(text)
        except (ValueError, UnicodeError) as e:
            problem(rel, "does not parse: %s" % e)
            continue
        searched = "\n".join(docs[rel]) if "/lang/" in rel else text
        found = DROPPED.search(searched)
        if found:
            problem(rel, "mentions %r, which was dropped" % found.group(0))
    elif rel.endswith(".png"):
        with open(path, "rb") as f:
            head = f.read(24)
        if head[:8] != b"\x89PNG\r\n\x1a\n":
            problem(rel, "not a PNG")
        elif "/textures/block/speaker_" in rel and struct.unpack(">II", head[16:24]) != (16, 16):
            problem(rel, "a speaker texture is 16 x 16")


def under(prefix, suffix=".json"):
    """The parsed files under a folder, by their name inside it."""
    return {rel[len(prefix):-len(suffix)]: doc for rel, doc in docs.items()
            if rel.startswith(prefix) and rel.endswith(suffix)}


def only(where, value, allowed):
    if not isinstance(value, dict):
        problem(where, "not an object")
        return False
    for key in value:
        if key not in allowed:
            problem(where, "field %r is not one Horizon reads (%s)" % (key, ", ".join(sorted(allowed))))
    return True


manifest_path = os.path.join(root, "horizon-mod.toml")
manifest = open(manifest_path, encoding="utf-8").read() if os.path.exists(manifest_path) else ""
for line in ('id = "%s"' % MOD, 'api = "0.2"', 'side = "both"', 'capabilities = ["registries"]'):
    if line not in manifest.splitlines():
        problem("horizon-mod.toml", "no line %s" % line)

# ---- declarations: data/<id>/horizon/{block,item,creative_tab}/
declared = under("data/%s/horizon/" % MOD)
for name in declared:
    if name.split("/")[0] not in ("block", "item", "creative_tab"):
        problem("data/%s/horizon/%s.json" % (MOD, name), "not in block/, item/ or creative_tab/")
blocks = {name[6:]: doc for name, doc in declared.items() if name.startswith("block/")}
plain_items = {name[5:]: doc for name, doc in declared.items() if name.startswith("item/")}
tabs = {name[13:]: doc for name, doc in declared.items() if name.startswith("creative_tab/")}

STATE_NAME = re.compile(r"[a-z0-9_]+")
block_properties = {}


def check_shapes(where, shapes, properties):
    if not only(where, shapes, {"property", "properties", "values", "default"}):
        return
    names = [shapes["property"]] if shapes.get("property") else list(shapes.get("properties", []))
    values, default = shapes.get("values", {}), shapes.get("default")
    if ("property" in shapes and "properties" in shapes) or len(set(names)) != len(names) \
            or (not names and values) or (not values and default is None):
        problem(where, "needs distinct properties and values, or a default")
    for name in names:
        if name not in properties:
            problem(where, "the block has no state property %r" % name)
    domains = [properties.get(name, []) for name in names]
    for key in values:
        parts = key.split(",")
        if len(parts) != len(names) or any(p not in d for p, d in zip(parts, domains)):
            problem(where, "%r is not one value per property" % key)
    if default is None:
        wanted = 1
        for domain in domains:
            wanted *= len(domain)
        if len(values) != wanted:
            problem(where, "without a default, every value needs its boxes (%d of %d)" % (len(values), wanted))
    for boxes in list(values.values()) + ([default] if default is not None else []):
        if len(boxes) > 16 or any(
                len(b) != 6 or any(not isinstance(v, (int, float)) for v in b)
                or any(b[axis] > b[axis + 3] for axis in range(3)) for b in boxes):
            problem(where, "a shape is at most 16 boxes [minX,minY,minZ,maxX,maxY,maxZ]")


def check_components(where, components):
    for key, value in components.items():
        kind = key[1:] if key.startswith("!") else key
        if kind not in COMPONENT_TYPES:
            problem(where, "component %r is not in COMPONENT_TYPES" % kind)
        if key.startswith("!") and value != {}:
            problem(where, "a removal's value is {}")


for name, block in sorted(blocks.items()):
    where = "data/%s/horizon/block/%s.json" % (MOD, name)
    if not only(where, block, {"template", "properties", "shapes", "collision_shapes", "state_properties",
                               "default_state", "connects_to", "thickness", "wood_type", "wall_block", "item",
                               "components"}):
        continue
    template = block.get("template")
    if template not in TEMPLATES:
        problem(where, "template %r is not in TEMPLATES" % template)
    properties = dict(TEMPLATES.get(template, {}))
    for key, own in block.get("state_properties", {}).items():
        if key in properties or not STATE_NAME.fullmatch(key):
            problem(where, "state property %r is the template's, or not a name" % key)
        if not only(where, own, {"type", "values"}) or own.get("type") not in ("boolean", "enum"):
            problem(where, "state property %r is a boolean or an enum" % key)
        properties[key] = BOOL if own.get("type") == "boolean" else own.get("values", [])
    block_properties[name] = properties
    for key, value in block.get("default_state", {}).items():
        if value not in properties.get(key, []):
            problem(where, "default_state %s=%s is not a state of the block" % (key, value))
    p = block.get("properties", {})
    if only(where + " properties", p, {"hardness", "explosion_resistance", "light", "requires_tool", "friction",
                                       "map_color", "sounds", "random_ticks", "render_shape", "occludes"}):
        if p.get("hardness", 0) < 0 and p["hardness"] != -1:
            problem(where, "hardness is 0 or more, or -1")
        if p.get("explosion_resistance", 0) < 0 or not 0 <= p.get("light", 0) <= 15 \
                or not 0 <= p.get("map_color", 0) <= 61 or not 0 <= p.get("friction", 0.6) <= 2:
            problem(where, "a number of properties is out of range")
        if p.get("render_shape", "MODEL") not in ("MODEL", "ENTITYBLOCK_ANIMATED", "INVISIBLE"):
            problem(where, "render_shape %r" % p["render_shape"])
        if "sounds" in p and p["sounds"] not in TEMPLATES:
            problem(where, "sounds %r is not a block in TEMPLATES" % p["sounds"])
        for key in ("requires_tool", "occludes"):
            if not isinstance(p.get(key, True), bool):
                problem(where, "%s is true or false" % key)
    for key in ("shapes", "collision_shapes"):
        if key in block:
            check_shapes("%s %s" % (where, key), block[key], properties)
    has_item = block.get("item", True)
    if not isinstance(has_item, bool):
        problem(where, "item is true or false")
    if block.get("components") and not has_item:
        problem(where, "components, but no item to take them")
    check_components(where, block.get("components", {}))
    if "connects_to" in block:
        tag = block["connects_to"]
        if template != "minecraft:chorus_plant" or not tag.startswith("#"):
            problem(where, "connects_to is a #tag, on template minecraft:chorus_plant")
        elif "data/%s/tags/block/%s.json" % tuple(tag[1:].split(":")) not in docs:
            problem(where, "connects_to %s is not a block tag of the mod" % tag)
    if "thickness" in block and ("connects_to" not in block or "shapes" in block):
        problem(where, "thickness is a connecting block's, in place of shapes")
    for key in ("wood_type", "wall_block"):
        if key in block:
            problem(where, "%s is a sign's" % key)

block_items = {name for name, block in blocks.items() if block.get("item", True)}
for name, item in sorted(plain_items.items()):
    where = "data/%s/horizon/item/%s.json" % (MOD, name)
    if not only(where, item, {"template", "max_stack_size", "rarity", "components", "crafting_remainder",
                              "place_block"}):
        continue
    if item.get("template") not in ITEM_TEMPLATES:
        problem(where, "template %r is not in ITEM_TEMPLATES" % item.get("template"))
    if not 1 <= item.get("max_stack_size", 64) <= 99:
        problem(where, "max_stack_size is 1 to 99")
    if item.get("rarity", "common") not in ("common", "uncommon", "rare", "epic"):
        problem(where, "rarity %r" % item["rarity"])
    if "place_block" in item and item["place_block"].split(":")[-1] not in blocks:
        problem(where, "place_block %r is not a block of the mod" % item["place_block"])
    if "crafting_remainder" in item:
        problem(where, "crafting_remainder: none is expected in this mod")
    check_components(where, item.get("components", {}))
    if name in block_items:
        problem(where, "also a block's item")
items = set(plain_items) | block_items
item_ids = {"%s:%s" % (MOD, name) for name in items}
block_ids = {"%s:%s" % (MOD, name) for name in blocks}


def is_item(name):
    return name in item_ids or name in VANILLA_ITEMS


listed = []
for name, tab in sorted(tabs.items()):
    where = "data/%s/horizon/creative_tab/%s.json" % (MOD, name)
    if only(where, tab, {"title", "icon", "items"}):
        for entry in [tab.get("icon")] + tab.get("items", []):
            if not isinstance(entry, str) or not is_item(entry):
                problem(where, "%r is not an item" % entry)
        listed += tab.get("items", [])
for name in sorted(item_ids):
    if listed.count(name) != 1:
        problem("creative tabs", "%s is listed %d times" % (name, listed.count(name)))

# ---- block states, models, textures
models = under("assets/%s/models/" % MOD)
blockstates = under("assets/%s/blockstates/" % MOD)
item_files = under("assets/%s/items/" % MOD)
used_models = set()


def model_ok(where, name):
    """Whether a model a block state or an item names exists."""
    name = full(name)
    namespace, path = name.split(":", 1)
    if namespace == MOD:
        used_models.add(path)
        if path not in models:
            problem(where, "model %s does not exist" % name)
    elif name not in VANILLA_MODELS:
        problem(where, "model %s is not in VANILLA_MODELS" % name)


def variants_of(where, value):
    for variant in value if isinstance(value, list) else [value]:
        if only(where, variant, {"model", "x", "y", "z", "uvlock", "weight"}):
            model_ok(where, variant.get("model", ""))
            if any(variant.get(axis, 0) % 90 for axis in "xyz"):
                problem(where, "a model is turned by quarter turns")


def condition_ok(where, when, properties):
    for key, value in when.items():
        if key in ("OR", "AND"):
            for term in value:
                condition_ok(where, term, properties)
        elif key not in properties:
            problem(where, "the block has no state property %r" % key)
        elif any(v.lstrip("!") not in properties[key] for v in str(value).split("|")):
            problem(where, "%s=%s is not a state of the block" % (key, value))


for name in sorted(blocks):
    where = "assets/%s/blockstates/%s.json" % (MOD, name)
    state = blockstates.get(name)
    if state is None:
        problem(where, "missing: the block has no block state")
        continue
    properties = block_properties.get(name, {})
    only(where, state, {"variants", "multipart"})
    for key, value in state.get("variants", {}).items():
        for clause in filter(None, key.split(",")):
            prop, _, value_of = clause.partition("=")
            if value_of not in properties.get(prop, []):
                problem(where, "variant %r: %s is not a state of the block" % (key, clause))
        variants_of(where, value)
    for part in state.get("multipart", []):
        if only(where, part, {"when", "apply"}):
            condition_ok(where, part.get("when", {}), properties)
            variants_of(where, part.get("apply", {}))
    # Every state draws something, unless nothing is to be drawn.
    if "variants" in state and "" not in state["variants"]:
        covered = set()
        for key in state["variants"]:
            covered.update(c for c in key.split(","))
        for prop in {c.partition("=")[0] for c in covered}:
            lacking = [v for v in properties.get(prop, []) if "%s=%s" % (prop, v) not in covered]
            if lacking:
                problem(where, "no variant for %s=%s" % (prop, "|".join(lacking)))
for name in sorted(set(blockstates) - set(blocks)):
    problem("assets/%s/blockstates/%s.json" % (MOD, name), "no such block")


def item_model_refs(where, node):
    """The models an items/ file names, wherever its tree of conditions puts them."""
    if isinstance(node, dict):
        if node.get("type") in ("minecraft:model", "model"):
            model_ok(where, node.get("model", ""))
        for value in node.values():
            item_model_refs(where, value)
    elif isinstance(node, list):
        for value in node:
            item_model_refs(where, value)


for name in sorted(items):
    where = "assets/%s/items/%s.json" % (MOD, name)
    if name not in item_files:
        problem(where, "missing: the item has no model definition")
    elif only(where, item_files[name], {"model", "hand_animation_on_swap", "oversized_in_gui", "swap_animation_scale"}):
        item_model_refs(where, item_files[name].get("model"))
for name in sorted(set(item_files) - items):
    problem("assets/%s/items/%s.json" % (MOD, name), "no such item")


def resolve(name, seen=()):
    """A model's texture slots after its parents', and the slots a vanilla parent leaves open."""
    where = "assets/%s/models/%s.json" % (MOD, name)
    model = models[name]
    slots, wanted = {}, set()
    parent = model.get("parent")
    if parent:
        parent = full(parent)
        namespace, path = parent.split(":", 1)
        if namespace == MOD:
            if path not in models:
                problem(where, "parent %s does not exist" % parent)
            elif path in seen:
                problem(where, "parent %s loops" % parent)
            else:
                used_models.add(path)
                slots, wanted = resolve(path, seen + (name,))
        elif parent in VANILLA_MODELS:
            wanted = set(VANILLA_MODELS[parent])
        else:
            problem(where, "parent %s is not in VANILLA_MODELS" % parent)
    slots = dict(slots, **model.get("textures", {}))
    for element in model.get("elements") or []:
        for face in element.get("faces", {}).values():
            wanted.add(face.get("texture", "").lstrip("#"))
    return slots, wanted


for name in sorted(models):
    where = "assets/%s/models/%s.json" % (MOD, name)
    slots, wanted = resolve(name)
    for slot, texture in sorted(slots.items()):
        if texture.startswith("#"):
            if texture[1:] not in slots:
                problem(where, "texture %s=%s names no slot" % (slot, texture))
            continue
        texture = full(texture)
        namespace, path = texture.split(":", 1)
        if namespace == MOD:
            if "assets/%s/textures/%s.png" % (MOD, path) not in files:
                problem(where, "texture %s does not exist" % texture)
        elif texture not in VANILLA_TEXTURES:
            problem(where, "texture %s is not in VANILLA_TEXTURES" % texture)
    for slot in sorted(wanted - set(slots)):
        problem(where, "texture slot #%s is never given" % slot)
for name in sorted(set(models) - used_models):
    problem("assets/%s/models/%s.json" % (MOD, name), "no block state, item or model uses it")

# ---- data: recipes, loot tables, tags, advancements
recipes = under("data/%s/recipe/" % MOD)
made = set()


def ingredient_ok(where, value):
    for entry in value if isinstance(value, list) else [value]:
        if not isinstance(entry, str):
            problem(where, "ingredient %r is not an id or a #tag (26.3)" % (entry,))
        elif entry.startswith("#"):
            if entry[1:] not in VANILLA_ITEM_TAGS:
                problem(where, "tag %s is not in VANILLA_ITEM_TAGS" % entry)
        elif not is_item(entry):
            problem(where, "ingredient %s is not an item" % entry)


for name, recipe in sorted(recipes.items()):
    where = "data/%s/recipe/%s.json" % (MOD, name)
    kind = recipe.get("type")
    if kind == "minecraft:crafting_shaped":
        only(where, recipe, {"type", "category", "group", "key", "pattern", "result", "show_notification"})
        pattern, key = recipe.get("pattern", []), recipe.get("key", {})
        if not 1 <= len(pattern) <= 3 or len({len(row) for row in pattern}) != 1 or not 1 <= len(pattern[0]) <= 3:
            problem(where, "pattern is 1 to 3 rows of one width, 1 to 3")
        if set("".join(pattern)) - {" "} != set(key):
            problem(where, "pattern and key do not use the same symbols")
        for value in key.values():
            ingredient_ok(where, value)
    elif kind == "minecraft:crafting_shapeless":
        only(where, recipe, {"type", "category", "group", "ingredients", "result"})
        for value in recipe.get("ingredients", []):
            ingredient_ok(where, value)
    else:
        problem(where, "type %r" % kind)
    if recipe.get("category", "misc") not in RECIPE_CATEGORIES:
        problem(where, "category %r" % recipe["category"])
    result = recipe.get("result")
    if only(where + " result", result, {"id", "count", "components"}):
        if result.get("id") not in item_ids:
            problem(where, "result %r is not an item of the mod" % result.get("id"))
        if not 1 <= result.get("count", 1) <= 64:
            problem(where, "count %r" % result.get("count"))
        made.add(result.get("id"))


def old_loot(node):
    """Whether a loot table still holds 1.21's `conditions` or `functions`."""
    if isinstance(node, dict):
        return "conditions" in node or "functions" in node or any(old_loot(v) for v in node.values())
    return isinstance(node, list) and any(old_loot(v) for v in node)


loot = under("data/%s/loot_table/blocks/" % MOD)
for name in sorted(blocks):
    where = "data/%s/loot_table/blocks/%s.json" % (MOD, name)
    table = loot.get(name)
    if table is None:
        problem(where, "missing: say what the block drops, if only nothing")
        continue
    if table.get("type") != "minecraft:block" or old_loot(table):
        problem(where, "a 26.3 block table has type minecraft:block, `condition` and `modifier`")
    drops = []
    for pool in table.get("pools", []):
        only(where + " pool", pool, {"rolls", "bonus_rolls", "entries", "condition", "modifier"})
        if pool.get("condition") != {"type": "minecraft:survives_explosion"}:
            problem(where, "a pool without the survives_explosion condition")
        for entry in pool.get("entries", []):
            if entry.get("type") != "minecraft:item" or entry.get("name") not in item_ids:
                problem(where, "entry %r is not an item of the mod" % entry.get("name"))
            drops.append(entry.get("name"))
    if name in block_items and drops != ["%s:%s" % (MOD, name)]:
        problem(where, "a block with an item drops it")
for name in sorted(set(loot) - set(blocks)):
    problem("data/%s/loot_table/blocks/%s.json" % (MOD, name), "no such block")

for rel, tag in sorted(docs.items()):
    match = re.fullmatch(r"data/([^/]+)/tags/([^/]+)/(.+)\.json", rel)
    if not match:
        continue
    only(rel, tag, {"values", "replace"})
    if tag.get("replace"):
        problem(rel, "replaces the tag for every mod")
    known = {"block": block_ids, "item": item_ids}.get(match.group(2))
    if known is None:
        problem(rel, "a tag of %s: only block and item tags are checked" % match.group(2))
        continue
    for value in tag.get("values", []):
        if value not in known:
            problem(rel, "%r is not one of the mod's" % (value,))
pickaxe = docs.get("data/minecraft/tags/block/mineable/pickaxe.json", {}).get("values", [])
for name in sorted(block_ids - set(pickaxe)):
    problem("data/minecraft/tags/block/mineable/pickaxe.json", "%s is missing" % name)

lang = under("assets/%s/lang/" % MOD)
english, french = lang.get("en_us", {}), lang.get("fr_fr", {})
advancements = under("data/%s/advancement/" % MOD)
custom_triggers = {}
for name, advancement in sorted(advancements.items()):
    where = "data/%s/advancement/%s.json" % (MOD, name)
    only(where, advancement, {"parent", "display", "criteria", "requirements", "rewards", "sends_telemetry_event"})
    parent = advancement.get("parent")
    if parent is not None and parent.split(":")[-1] not in advancements:
        problem(where, "parent %s does not exist" % parent)
    display = advancement.get("display", {})
    if only(where + " display", display, {"icon", "title", "description", "frame", "background", "show_toast",
                                          "announce_to_chat", "hidden"}):
        if not is_item(display.get("icon", {}).get("id", "")) or set(display.get("icon", {})) - {"id", "count", "components"}:
            problem(where, "icon %r is not {\"id\": an item}" % (display.get("icon"),))
        for key in ("title", "description"):
            text = display.get(key, {}).get("translate")
            if text not in english or text not in french:
                problem(where, "%s %r is not in both languages" % (key, text))
        if display.get("frame", "task") not in ("task", "goal", "challenge"):
            problem(where, "frame %r" % display["frame"])
        if (parent is None) != ("background" in display):
            problem(where, "the root, and only the root, has a background")
        if "background" in display and display["background"] not in VANILLA_BACKGROUNDS:
            problem(where, "background %r is not in VANILLA_BACKGROUNDS" % display["background"])
    criteria = advancement.get("criteria", {})
    if not criteria:
        problem(where, "no criteria")
    for criterion, value in criteria.items():
        only(where + " " + criterion, value, {"trigger", "conditions"})
        trigger = value.get("trigger", "")
        if trigger.startswith(MOD + ":"):
            # The native mod fires it; the game tests nothing but a `player` predicate.
            custom_triggers.setdefault(trigger, []).append("%s:%s/%s" % (MOD, name, criterion))
        elif trigger == "minecraft:inventory_changed":
            for predicate in value.get("conditions", {}).get("items", []):
                for item in [predicate["items"]] if isinstance(predicate.get("items"), str) else predicate.get("items", []):
                    if not is_item(item):
                        problem(where, "%s holds %r, not an item" % (criterion, item))
        else:
            problem(where, "trigger %r is neither the mod's nor one this script knows" % trigger)
    for group in advancement.get("requirements", []):
        for criterion in group:
            if criterion not in criteria:
                problem(where, "requirements name %r, not a criterion" % criterion)
    rewards = advancement.get("rewards", {})
    only(where + " rewards", rewards, {"recipes", "experience", "loot", "function"})
    for recipe in rewards.get("recipes", []):
        if recipe.split(":")[-1] not in recipes or not recipe.startswith(MOD + ":"):
            problem(where, "rewards the recipe %s, which does not exist" % recipe)
unlocked = [r for a in advancements.values() for r in a.get("rewards", {}).get("recipes", [])]
for name in sorted(recipes):
    if "%s:%s" % (MOD, name) not in unlocked:
        problem("data/%s/recipe/%s.json" % (MOD, name), "no advancement unlocks it")

# ---- sounds and languages
sounds = docs.get("assets/%s/sounds.json" % MOD, {})
played = set()
for event, entry in sorted(sounds.items()):
    where = "assets/%s/sounds.json %s" % (MOD, event)
    if only(where, entry, {"sounds", "subtitle", "replace"}):
        if not entry.get("sounds"):
            problem(where, "no sounds")
        for sound in entry.get("sounds", []):
            name = full(sound["name"] if isinstance(sound, dict) else sound)
            path = "assets/%s/sounds/%s.ogg" % tuple(name.split(":", 1))
            played.add(path)
            if path not in files:
                problem(where, "%s does not exist" % path)
for rel in sorted(files):
    if rel.endswith(".ogg") and rel not in played:
        problem(rel, "no sound event plays it")

for code in ("en_us", "fr_fr"):
    if code not in lang:
        problem("assets/%s/lang/%s.json" % (MOD, code), "missing")
for key in sorted(set(english) ^ set(french)):
    problem("assets/%s/lang" % MOD, "%s is in only one of en_us and fr_fr" % key)
PLACEHOLDER = re.compile(r"%(?:\d+\$)?[sd]")
for code, text in sorted(lang.items()):
    where = "assets/%s/lang/%s.json" % (MOD, code)
    for key, value in text.items():
        if not isinstance(value, str) or not value:
            problem(where, "%s has no text" % key)
        elif key not in english:
            problem(where, "%s is not an English key" % key)
        elif sorted(PLACEHOLDER.findall(value)) != sorted(PLACEHOLDER.findall(english[key])):
            problem(where, "%s does not take what the English text takes" % key)
    named = re.compile(r"(block|item)\.%s\.([a-z0-9_]+)$" % MOD)
    for key in text:
        match = named.match(key)
        # `item.webdisplays.wiki` is a tooltip, the one such key that names no item.
        if match and key != "item.%s.wiki" % MOD and match.group(2) not in (blocks if match.group(1) == "block" else items):
            problem(where, "%s names no %s" % (key, match.group(1)))
for code in ("en_us", "fr_fr"):
    text = lang.get(code, {})
    where = "assets/%s/lang/%s.json" % (MOD, code)
    for name in sorted(blocks):
        if "block.%s.%s" % (MOD, name) not in text:
            problem(where, "no name for the block %s" % name)
    for name in sorted(plain_items):
        if "item.%s.%s" % (MOD, name) not in text:
            problem(where, "no name for the item %s" % name)
    for name, tab in tabs.items():
        if tab.get("title") not in text and "itemGroup.%s.%s" % (MOD, name) not in text:
            problem(where, "no title for the creative tab %s" % name)


def labels(node):
    """The `$key` texts of a Java GUI layout (assets/<id>/gui/), which the native mod reads."""
    if isinstance(node, str):
        return [node[1:]] if node.startswith("$") else []
    return [key for value in (node.values() if isinstance(node, dict) else node if isinstance(node, list) else [])
            for key in labels(value)]


for name, layout in sorted(under("assets/%s/gui/" % MOD).items()):
    for key in labels(layout):
        if key not in english:
            problem("assets/%s/gui/%s.json" % (MOD, name), "label $%s is not a language key" % key)

# ---- the native mod's sentences. Its Rust sources name each one by its key, beside the
# English text a client without these files shows: `Phrase::new("webdisplays...", "...")`
# (native/api/src/model/phrase.rs). Every key used there must be in both languages, with
# the English of the code; every `webdisplays.h.` key must be used there; and each text
# must be one the game fills (Horizon's `format_translation` knows `%s`, `%1$s` and `%%`
# only, and shows a template that holds anything else as it is written).
NATIVE = os.path.join(HERE, "native", "api", "src")
# HCEF, the browser mod: this mod shows its sentences and so translates them. Its
# repository is expected beside this one (HCEF_REPO says otherwise).
HCEF = os.path.join(os.environ.get("HCEF_REPO") or os.path.join(os.path.dirname(HERE), "HCEF"), "native", "api", "src")
USED = re.compile(r'"((?:%s|hcef)\.[A-Za-z0-9_.]+)"(?:\s*,\s*"((?:[^"\\]|\\.)*)")?' % MOD)
OWN = "%s.h." % MOD
PLACE = re.compile(r"%(?:(\d+)\$)?")


def rust_text(literal):
    """What a Rust string literal says: its escapes as the characters they stand for."""
    def escape(match):
        code = match.group(1)
        return chr(int(code[2:-1], 16)) if code.startswith("u{") else {"n": "\n", "t": "\t"}.get(code, code)
    return re.sub(r"\\(u\{[0-9a-fA-F]+\}|.)", escape, literal)


def fills(template):
    """How many arguments the game puts into `template`; None when it would show it unfilled."""
    plain = numbered = at = 0
    while at < len(template):
        if template[at] != "%":
            at += 1
            continue
        match = PLACE.match(template, at)
        number, conversion = match.group(1), template[match.end():match.end() + 1]
        if conversion == "%" and number is None:
            at = match.end() + 1
        elif conversion and conversion != "%" and not (conversion.isascii() and conversion.isalpha()):
            # Not a placeholder at all: the percent sign is itself.
            at += 1
        elif conversion == "s" and (number is None or int(number) > 0):
            plain += number is None
            numbered = max(numbered, int(number or 0))
            at = match.end() + 1
        else:
            return None
    return max(plain, numbered)


used = {}
if not os.path.isdir(HCEF):
    problem("tools/check_content.py", "no HCEF sources at %s: its sentences are not checked" % HCEF)
for folder, _, names in [entry for root in (NATIVE, HCEF) for entry in os.walk(root)]:
    for name in sorted(names):
        if not name.endswith(".rs"):
            continue
        where = os.path.relpath(os.path.join(folder, name), os.path.dirname(HERE)).replace(os.sep, "/")
        with open(os.path.join(folder, name), encoding="utf-8") as f:
            # A file's tests come last, and make up keys of their own.
            source = f.read().split("#[cfg(test)]")[0]
        for match in USED.finditer(source):
            key, text = match.group(1), match.group(2)
            used.setdefault(key, where)
            for code, known in (("en_us", english), ("fr_fr", french)):
                if key not in known:
                    problem(where, "%s is not in %s.json: it goes in LANG_NATIVE of mkcontent.py" % (key, code))
            if text is None:
                problem(where, "%s is not beside its English text" % key)
            elif key in english and rust_text(text) != english[key]:
                problem(where, "%s is %a here and %a in en_us.json" % (key, rust_text(text), english[key]))
for key in sorted(english):
    if key.startswith(OWN) and used and key not in used:
        problem("assets/%s/lang/en_us.json" % MOD, "%s is used nowhere in native/api/src" % key)
for code, text in sorted(lang.items()):
    for key in sorted(set(text) & set(used)):
        taken = fills(text[key])
        if taken is None:
            problem("assets/%s/lang/%s.json" % (MOD, code), "%s holds a %% the game does not fill (it knows %%s, %%1$s and %%%%)" % key)
        elif key in english and taken != fills(english[key]):
            problem("assets/%s/lang/%s.json" % (MOD, code), "%s does not take as many arguments as the English text" % key)

# The windows' labels sit in boxes of a fixed size (native/api/src/client/gui): a
# translation must fit the box the English label was laid out in, at about 6 pixels a
# character. key: (characters, what stands for each %s). English is what the windows were
# drawn and looked at with, and is not measured here.
FITS = {
    # config.rs: labels of 40 px before a field, buttons of 40 and 50 px beside one
    "webdisplays.h.gui.name": (6, ""), "webdisplays.h.gui.pixels": (6, ""), "webdisplays.h.config.volume": (6, ""),
    "webdisplays.h.config.link": (6, ""), "webdisplays.h.config.friends": (6, ""), "webdisplays.gui.screencfg.others": (6, ""),
    "webdisplays.h.config.set": (6, ""), "webdisplays.h.config.add": (6, ""),
    "webdisplays.h.config.remove": (8, ""), "webdisplays.h.config.take_out": (8, ""),
    "webdisplays.h.config.percent": (5, "100"),
    # config.rs: half a column (96 px), a column (196 px), beside a checkbox (180 px)
    "webdisplays.h.config.best_fit": (16, ""), "webdisplays.h.config.turned": (16, "270"),
    "webdisplays.h.config.shared": (16, ""), "webdisplays.h.config.shared_on": (16, ""), "webdisplays.h.config.all_screens": (16, ""),
    "webdisplays.h.config.edges.blocks": (32, ""), "webdisplays.h.config.edges.cut": (32, ""),
    "webdisplays.h.config.edges.slanted": (32, ""), "webdisplays.h.config.link_mode.joined": (32, ""),
    "webdisplays.h.config.link_mode.spaced": (32, ""),
    "webdisplays.h.config.oled": (30, ""), "webdisplays.h.config.auto_volume": (30, ""),
    "webdisplays.h.config.rounded": (18, ""), "webdisplays.h.config.flip": (16, ""),
    # config.rs: the rights, up to the first column of checkboxes (120 px), and what is listed
    "webdisplays.h.config.may": (18, ""), "webdisplays.h.config.right.url": (20, ""), "webdisplays.h.config.right.interact": (20, ""),
    "webdisplays.h.config.right.settings": (20, ""), "webdisplays.h.config.right.upgrades": (20, ""),
    "webdisplays.gui.screencfg.friendlist": (20, ""), "webdisplays.h.config.right.others": (20, ""),
    "webdisplays.h.config.upgrade.laser": (22, ""), "webdisplays.h.config.more_friends": (32, "13"),
    # every window's "Close", the narrowest being a quarter of the browsers' panel (91 px)
    "webdisplays.h.gui.close": (15, ""),
    # browsers.rs: a row's buttons (64, 56 and 56 px), the bottom row (91 px), the form
    "webdisplays.h.browsers.show": (10, ""), "webdisplays.h.browsers.shown": (10, ""), "webdisplays.h.browsers.change": (9, ""),
    "webdisplays.gui.seturl.browser.delete": (9, ""), "webdisplays.gui.seturl.browser.new": (15, ""),
    "webdisplays.h.browsers.own": (15, ""), "webdisplays.h.browsers.more": (15, ""),
    "webdisplays.h.browsers.form.address": (8, ""), "webdisplays.h.browsers.back": (31, ""),
    "webdisplays.h.browsers.title": (64, ""), "webdisplays.h.browsers.none": (64, ""), "webdisplays.h.browsers.bad_size": (64, ""),
    "webdisplays.h.browsers.form.new": (64, ""), "webdisplays.h.browsers.form.change": (64, ""),
    # url.rs: a panel of 280 px (264 inside), three buttons of 84 px
    "webdisplays.h.url.title": (44, ""), "webdisplays.h.url.hint": (44, ""), "webdisplays.h.url.empty": (44, ""),
    "webdisplays.h.address.too_long": (44, ""), "webdisplays.h.address.malformed": (44, ""),
    "webdisplays.h.url.open": (14, ""), "webdisplays.gui.seturl.cancel": (14, ""),
    # redstone.rs: a panel of 300 px (284 inside), two buttons of 139 px
    "webdisplays.h.redstone.title": (47, ""), "webdisplays.h.redstone.rising": (47, ""), "webdisplays.h.redstone.falling": (47, ""),
    "webdisplays.h.redstone.refused": (47, ""), "webdisplays.h.gui.save": (23, ""),
    # screens.rs: a row's button (50 px), the bottom row (127 px)
    "webdisplays.h.screens.this": (8, ""), "webdisplays.h.screens.join": (8, ""),
    "webdisplays.h.screens.previous": (21, ""), "webdisplays.h.screens.next": (21, ""),
    "webdisplays.h.screens.title": (65, ""), "webdisplays.h.screens.none": (65, ""),
    # the minePad's "Go" (42 px) and the keyboard's line, across the smallest window (426 px)
    "webdisplays.h.pad.go": (7, ""), "webdisplays.h.keyboard.typing": (68, ""),
}
for key in sorted(set(FITS) - set(english)):
    problem("FITS", "%s is not a language key" % key)
for code, text in sorted(lang.items()):
    for key, (room, sample) in sorted(FITS.items()):
        shown = text.get(key, "").replace("%s", sample).replace("%%", "%")
        if code != "en_us" and len(shown) > room:
            problem("assets/%s/lang/%s.json" % (MOD, code), "%s is %d characters (%a), its box holds %d" % (key, len(shown), shown, room))

# ---- the lists above against the game's own tables, when they are at hand
if vanilla is None:
    found = glob.glob(os.path.join(os.environ.get("APPDATA", ""), "horizon-launcher", "versions", "*", ".local",
                                   "horizon-client", "26.3", "facts.json"))
    vanilla = os.path.dirname(max(found, key=os.path.getmtime)) if found else None
if vanilla and not os.path.exists(os.path.join(vanilla, "facts.json")):
    sys.exit("%s is not a prepared 26.3 root: no facts.json" % vanilla)
if vanilla:
    def table(name):
        with open(os.path.join(vanilla, name), encoding="utf-8") as f:
            return json.load(f)

    game_items = table("items.json")
    for name in sorted(VANILLA_ITEMS | ITEM_TEMPLATES):
        if name not in game_items:
            problem("vanilla", "%s is not an item of the game" % name)
    known_components = {key for entry in game_items.values() for key in entry}
    for name in sorted(COMPONENT_TYPES - known_components):
        problem("vanilla", "%s is not a component type of the game" % name)
    states = {}
    for row in table("facts.json")["states"]:
        if row[0] in TEMPLATES:
            for key, value in row[4].get("properties", {}).items():
                states.setdefault(row[0], {}).setdefault(key, set()).add(value)
            states.setdefault(row[0], {})
    for name, properties in sorted(TEMPLATES.items()):
        if name not in states:
            problem("vanilla", "%s is not a block of the game" % name)
        elif {k: sorted(v) for k, v in states[name].items()} != {k: sorted(v) for k, v in properties.items()}:
            problem("vanilla", "%s has the state properties %s" % (name, states[name]))
    for name in sorted(VANILLA_ITEM_TAGS):
        if not os.path.exists(os.path.join(vanilla, "data", "minecraft", "tags", "item", name.split(":")[1] + ".json")):
            problem("vanilla", "#%s is not an item tag of the game" % name)
    for name in sorted(VANILLA_MODELS):
        if not os.path.exists(os.path.join(vanilla, "assets", "minecraft", "models", name.split(":")[1] + ".json")):
            problem("vanilla", "%s is not a model of the game" % name)
    for name in sorted(VANILLA_BACKGROUNDS):
        if not os.path.exists(os.path.join(vanilla, "assets", "minecraft", "textures", name.split(":")[1] + ".png")):
            problem("vanilla", "%s is not a texture of the game" % name)

for line in problems:
    print(line)
print("%d files: %d blocks, %d items, %d models, %d recipes, %d advancements, %d sound events, %d language keys"
      % (len(files), len(blocks), len(items), len(models), len(recipes), len(advancements), len(sounds), len(english)))
own = sum(key.startswith(OWN) for key in used)
print("the native mod says %d sentences: %d keys of its own, %d of the Java mod's" % (len(used), own, len(used) - own) if used
      else "no native mod at %s: its sentences were not checked" % NATIVE)
for trigger, where in sorted(custom_triggers.items()):
    print("the native mod fires %s for %s" % (trigger, ", ".join(where)))
print("vanilla names checked against %s" % vanilla if vanilla
      else "no prepared 26.3 root found: vanilla names checked against the lists in this script only")
if problems:
    sys.exit("%d problems" % len(problems))
print("ok")
