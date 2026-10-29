# Validation

On a MacBook Pro (Apple M5, 16 GB of memory, 1 TB of storage), macOS with Metal, Rust 1.97.1, 2026-10-29 and 30. The first runs below were on Horizon `7ebf7d2` (main
at the time); the mod was then re-pinned to the Nightly `d009ebd`, 48 commits older, which still runs WebAssembly
mods, and checked again there (last section). horizon-mixins `0d6b103` throughout.

| Check | Result |
|---|---|
| `cargo test` | 14 tests pass (13 repeated 12 times: no flakes) |
| `cargo clippy --all-targets -- -D warnings` | Pass |
| `python3 tools/horizon.py check` | Horizon with the bundle applied type-checks; no warning from the mod's code |
| `python3 tools/horizon.py test` | The same 13 tests pass inside Horizon's game test binary |
| `python3 tools/horizon.py build` | `horizon-client` (fast profile) builds in about 2 minutes after a warm build |
| `python3 tools/proof.py` | Pass (below) |

The proof ran the built game on resources prepared from Minecraft 26.3, in a new world (fresh temporary game,
launcher and XDG folders; no player data touched), render distance 12, LOD distance 256 chunks, Medium quality:

- Options > Mods lists "Distant Horizons 0.1.0: ON"; its settings page shows the six settings.
- 458 of 458 planned sections drawn 111 s after joining: 457 sections generated from the world generator, 1290
  chunk reads; 9.6 million faces.
- The horizon band of the screen differs by 11.3 (mean colour difference, 0-255) between DH on and off.
- 524 section files saved under the world's `DistantHorizons/overworld/`.

A second run at render distance 32 (a player's report: at a large render distance the queue grew past 12,000
jobs and nothing was drawn): 3,772 loaded chunks read in the first 15 s, 345 of 345 sections drawn after about
2.5 minutes, the queue empty, 39 fps. The fix: chunk updates now go to their leaf section only and coarser levels
take them in batched jobs (as `FullDataUpdatePropagatorV2` does), all work is ordered by distance, no lock is held
while sections merge, only changed columns are checked for hidden runs, and a chunk is read again only when its
blocks change. The engine test now loads 625 chunks at once and checks every section is still drawn and the
chunks reach the coarsest level.

On `d009ebd` (the current pin), with the WebAssembly runtime built in (`--features horizon-client/mods`, as the
launcher builds):

- The bundle applies and compiles unchanged; the 13 tests pass inside the game's test binary, alone and with
  Horizon's six official native mods built in (`--with`).
- In a running game with all seven native mods and Mod Menu (a WebAssembly mod): Options > Mods lists the eight,
  all on; Coordinates HUD, FPS Counter, Keystrokes and Minimap draw beside the far terrain; 451 of 458 sections
  drawn within about 3 minutes while generation finished.
- Through the launcher itself (`horizon-launcher native-build` of a set with Zoom and FPS Counter, release profile,
  the launcher's Rust 1.99.1): built and played by a player; `/dh` works.
- Tab completion: `/dh qu` completes to `quality`, then its five values are offered (screenshot checked).
- Shutting a session down finishes the changes still on their way to the coarser levels before the last save.

Known limits seen in the runs:

- **Cost**: 9.6 million faces for 458 sections at 256 chunks (about 300 MB of vertex data); the frame rate fell from
  about 120 to 27 fps on this machine. Distant Horizons draws far fewer vertices per face (its own compact vertex
  format and shader); Horizon's terrain vertex is 24 bytes. Lower distances or quality help.
- **Water at section and near-area edges** shows translucent side walls (see-through water boxes' sides).
- Not tested: multiplayer servers, the Nether and the End, very long sessions, other Horizon revisions.
