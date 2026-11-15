"""Checks a crate's Cargo.lock against Horizon's: the allow list of docs/native-mods.md.

usage: python -I locked.py <Horizon's Cargo.lock> <the crate's Cargo.lock>

Every package the crate downloads (one with a `source`) must be in Horizon's lock with the
same name, version and source. Exits 1 and names the strangers when one is not.
"""
import re
import sys


def packages(path):
    """(name, version, source) of each downloaded package in a lock file."""
    found = set()
    for block in open(path, encoding="utf-8").read().split("[[package]]")[1:]:
        field = lambda key: (re.search(r'^%s = "([^"]*)"' % key, block, re.M) or [None, None])[1]
        if field("source"):
            found.add((field("name"), field("version"), field("source")))
    return found


horizon, ours = packages(sys.argv[1]), packages(sys.argv[2])
strangers = sorted(ours - horizon)
if strangers:
    print("NOT in Horizon's lock (the launcher would refuse the build):", file=sys.stderr)
    for name, version, source in strangers:
        print("  %s %s  (%s)" % (name, version, source), file=sys.stderr)
    sys.exit(1)
print("allow list: OK, all %d downloaded packages are in Horizon's lock" % len(ours))
