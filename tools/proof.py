#!/usr/bin/env python3
"""Distant Horizons in a running modded game, on fresh data (tools/game.py): the mod is listed in Options > Mods
with its settings page, its LODs are generated, meshed and drawn past the game's render distance, the view differs
with them off, and their sections are saved in the world.

    python3 tools/proof.py --resources /path/to/.local/horizon-client/26.3 [--out target/proof]
"""
import argparse
import json
import re
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from game import Game, difference  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
CTL = ROOT / "target" / "cargo" / "fast" / "horizon-ctl"
STATUS = re.compile(r"Distant Horizons [\d.]+: (\d+) of (\d+) sections drawn \((\d+) faces\), (\d+) jobs queued, "
                    r"(\d+) in memory, (\d+) generated, (\d+) chunks read; generator (\w+)")
# The sky band over the horizon, where only far terrain can be.
HORIZON = (0.0, 0.25, 1.0, 0.5)


def dh(g, args=""):
    """/dh typed in the chat box, as a player does: `horizon-ctl chat` hands the line to the single player
    server, which has no session."""
    g.run("key", "t", "tap")
    time.sleep(0.4)
    g.run("type", f"/dh {args}".strip())
    g.run("key", "enter", "tap")
    time.sleep(0.6)


def status(g):
    dh(g)
    for found in reversed(list(STATUS.finditer(g.run("logs")))):
        *numbers, generator = found.groups()
        keys = ["shown", "wanted", "faces", "queued", "loaded", "generated", "chunks"]
        return {**dict(zip(keys, map(int, numbers))), "generator": generator}
    return None


def run(resources, out):
    with Game(CTL, resources, out) as g:
        g.run("options", "mods")
        labels = g.labels()
        assert "Distant Horizons 0.1.0: ON" in labels, labels
        g.run("click", g.ids()["Settings"])
        page = g.labels()
        assert page[:3] == ["Distant Horizons: ON", "LOD render distance: 256 chunks", "Horizontal quality: Medium"], page
        g.shot("settings-page")
        g.play()
        # High above the terrain, looking out over it.
        g.run("command", "gamemode spectator")
        g.run("command", "tp @s 0 220 0 0 15")
        g.run("key", "f1", "tap")
        start = time.time()
        s = None
        while time.time() - start < 300:
            s = status(g)
            if s and s["wanted"] > 0 and s["shown"] >= s["wanted"] * 0.95 and s["queued"] == 0:
                break
            time.sleep(3)
        made = time.time() - start
        assert s and s["shown"] > 0 and s["generator"] == "on" and s["generated"] > 0, s
        time.sleep(2)
        on = g.shot("dh-on")
        dh(g, "debug on")
        time.sleep(4)
        g.shot("dh-debug")
        dh(g, "debug off")
        dh(g, "off")
        time.sleep(3)
        off = g.shot("dh-off")
        dh(g, "on")
        g.run("save")
        time.sleep(2)
        saves = list(g.home.glob("saves/**/DistantHorizons/overworld/*/*.dhs"))
        results = {
            "status": s,
            "seconds_to_draw": round(made, 1),
            "diff_horizon_on_off": round(difference(on, off, HORIZON), 1),
            "section_files": len(saves),
        }
        assert results["diff_horizon_on_off"] > 4, results
        assert results["section_files"] > 0, results
        print(json.dumps({"proof": "distant_horizons", "ok": True, **results}, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--resources", required=True, help="prepared Horizon resources for Minecraft 26.3")
    parser.add_argument("--out", default=str(ROOT / "target" / "proof"), help="where the screenshots go")
    args = parser.parse_args()
    run(args.resources, args.out)


if __name__ == "__main__":
    main()
