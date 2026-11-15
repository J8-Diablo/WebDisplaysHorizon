"""Packs WebDisplays for Horizon, and optionally installs it into an instance.

usage: python -I tools/pack.py [--out DIR] [--install INSTANCE_FOLDER] [--mixins COMMIT]

Writes into DIR (default: dist/ at the repository's root):

  webdisplays.hmod          the content mod: `content/` zipped, `horizon-mod.toml` at its root
  webdisplays_native.hnmod  the native mod's source (.rs and .toml only), which the launcher
                            compiles into the game (`horizon-launcher native-build`)

With --install, the content mod is also copied into the instance's `horizon-mods/`.

The browser is another mod's, HCEF, which the native mod depends on: its package and the
browser process are packed and installed from its own repository (HCEF/tools/pack.py).

Nothing is downloaded and nothing outside DIR and the instance folder is written.
"""
import argparse
import json
import os
import re
import shutil
import sys
import zipfile

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
def zip_tree(source, out, prefix=""):
    """Zips every file under `source`, sorted, with fixed times: the same tree gives the same file."""
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for root, dirs, files in os.walk(source):
            dirs.sort()
            for name in sorted(files):
                path = os.path.join(root, name)
                rel = prefix + os.path.relpath(path, source).replace(os.sep, "/")
                info = zipfile.ZipInfo(rel, date_time=(2026, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                with open(path, "rb") as f:
                    z.writestr(info, f.read())


def pack_content(out_dir):
    out = os.path.join(out_dir, "webdisplays.hmod")
    zip_tree(os.path.join(HERE, "content"), out)
    return out


def put(z, name, data):
    """Adds a file with a fixed time: the same sources pack to the same bytes, so a SHA-256
    someone pinned (a registry entry, a modpack) stays the file's."""
    info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_DEFLATED
    z.writestr(info, data)


def pack_native(out_dir, mixins):
    source = os.path.join(HERE, "native")
    manifest = open(os.path.join(source, "horizon-mixin.toml"), encoding="utf-8").read()
    mod_id = re.search(r'^id = "([a-z0-9_]+)"', manifest, re.M).group(1)
    version = re.search(r'^version = "([^"]+)"', manifest, re.M).group(1)
    horizon = re.search(r'^horizon_revision = "([0-9a-f]{40})"', manifest, re.M).group(1)
    out = os.path.join(out_dir, "%s.hnmod" % mod_id)
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        header = {"schema": 1, "id": mod_id, "version": version, "game": "26.3", "horizon": horizon}
        if mixins:
            header["mixins"] = mixins
        put(z, "horizon-native.json", (json.dumps(header, indent=2, sort_keys=True) + "\n").encode("utf-8"))
        for root, dirs, files in os.walk(source):
            # Not the build output, nor what tools/check.sh stages (native/.check).
            dirs[:] = sorted(d for d in dirs if d != "target" and not d.startswith("."))
            for name in sorted(files):
                # A bundle holds .rs and .toml files only.
                if name.endswith((".rs", ".toml")):
                    path = os.path.join(root, name)
                    rel = os.path.relpath(path, source).replace(os.sep, "/")
                    with open(path, "rb") as f:
                        # With LF, whatever the checkout has: line endings must not change the bundle.
                        put(z, "mixins/%s/%s" % (mod_id, rel), f.read().replace(b"\r\n", b"\n"))
    return out


def install(instance, content):
    mods = os.path.join(instance, "horizon-mods")
    if not os.path.isdir(instance):
        sys.exit("no instance folder at %s" % instance)
    os.makedirs(mods, exist_ok=True)
    shutil.copyfile(content, os.path.join(mods, os.path.basename(content)))
    print("installed: %s" % os.path.join(mods, os.path.basename(content)))


def main():
    parser = argparse.ArgumentParser(description="Packs WebDisplays for Horizon.")
    parser.add_argument("--out", default=os.path.join(HERE, "dist"))
    parser.add_argument("--install", metavar="INSTANCE_FOLDER")
    # The registry refuses a bundle that does not name its horizon-mixins commit. This is the one
    # the launcher uses for the pinned Horizon revision, which the mod was built and tested with.
    parser.add_argument("--mixins", default="5a64af938ec86b00d53d16a408aabc0038fb3e57", help="the horizon-mixins commit the native mod was checked against")
    args = parser.parse_args()
    os.makedirs(args.out, exist_ok=True)
    content = pack_content(args.out)
    native = pack_native(args.out, args.mixins)
    print(content)
    print(native)
    if args.install:
        install(args.install, content)


if __name__ == "__main__":
    main()
