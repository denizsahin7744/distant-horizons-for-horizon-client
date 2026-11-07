# Validation

## Single player (0.1.0)

MacBook Pro (Apple M5, 16 GB, 1 TB), macOS, Metal, Rust 1.97.1, 2026-10-29/30. First on Horizon `7ebf7d2` (then
main), then re-pinned to Nightly `d009ebd` (48 commits older, still runs WebAssembly mods) and rechecked.
horizon-mixins `0d6b103` throughout.

| Check | Result |
|---|---|
| `cargo test` | 14 pass (13 repeated 12 times: no flakes) |
| `cargo clippy --all-targets -- -D warnings` | Pass |
| `horizon.py check` | Type-checks with the bundle; no warnings from the mod |
| `horizon.py test` | The 13 tests pass in the game's test binary |
| `horizon.py build` | `horizon-client` (fast) in about 2 min warm |
| `tools/proof.py` | Pass (below) |

**Proof** (new world, fresh temporary game/launcher/XDG folders, render distance 12, LOD 256 chunks, Medium):

- Options > Mods: "Distant Horizons 0.1.0: ON"; settings page shows six settings.
- 458/458 sections drawn 111 s after joining: 457 generated, 1,290 chunk reads, 9.6 M faces.
- Horizon band differs by 11.3 (mean colour difference, 0-255) between on and off.
- 524 section files under `DistantHorizons/overworld/`.

**Render distance 32** (reported: queue past 12,000 jobs, nothing drawn): 3,772 chunks read in 15 s, 345/345 drawn
after about 2.5 min, queue empty, 39 fps. Fix: chunk updates go to the leaf only, coarser levels batched (as
`FullDataUpdatePropagatorV2`); all work ordered by distance; no lock held during merges; only changed columns
re-culled; chunks re-read only when blocks change. The engine test now loads 625 chunks at once and checks all
sections drawn and the coarsest level reached.

**On `d009ebd`** with the WebAssembly runtime (`--features horizon-client/mods`, as the launcher builds):

- Bundle applies unchanged; 13 tests pass in the game's test binary, alone and with Horizon's six official native
  mods (`--with`).
- Running game with all seven native mods and Mod Menu: Options > Mods lists eight, all on; Coordinates HUD, FPS
  Counter, Keystrokes, Minimap draw beside the far terrain; 451/458 sections in about 3 min while generating.
- Via `horizon-launcher native-build` (set with Zoom and FPS Counter, release profile, launcher Rust 1.99.1):
  built and played by a player; `/dh` works.
- Tab: `/dh qu` completes to `quality`, then offers its five values (screenshot checked).
- Shutdown finishes pending coarser-level changes before the last save.

**Known limits**:

- **Cost**: 9.6 M faces for 458 sections at 256 chunks (about 300 MB vertex data); about 120 to 27 fps here.
  Distant Horizons uses a compact vertex format and shader; Horizon's terrain vertex is 24 bytes. Lower distance
  or quality helps.
- **Water at section and near-area edges**: translucent side walls.
- Not tested: the End, very long sessions, other Horizon revisions.

## Servers (unreleased)

Linux (Ubuntu 24.04, kernel 6.8), game under Xvfb on Mesa llvmpipe Vulkan, rustup Rust, Horizon `d009ebd`,
horizon-mixins `0d6b103`, 2026-11-06/07.

### Fabric, Distant Horizons 3.3.4 (protocol 16)

Offline mode, same machine.

| Check | Result |
|---|---|
| `cargo test` | 28 pass, incl. codecs against reference-compressor fixtures (zstandard, lz4, Python lzma) |
| `cargo clippy --all-targets -- -D warnings` | Pass |
| `horizon.py check`, `test` | No warnings; 28 pass in the game's test binary |
| `horizon.py interop` | Pass with each compression mode (below) |
| `tools/server_proof.py` | Pass (below) |

**Interop** (Horizon's connection, no game), `dataCompression` set to uncompressed, LZ4, Zstandard (block), LZMA2
in turn: session configured, level named, 26 sections around the player plus real-time pushes, all decoded, none
failed; 1,100 to 1,350 distinct biome/block entries, every block name parsed. Re-asked with the current time (as
sync on load): 26/26 up to date. Distant Horizons 3.3.4 won't write the deprecated Zstandard stream mode; that
decoder is covered by fixtures only.

**In game** (`server_proof.py`, fresh folders, render distance 12, LOD 256 chunks, Medium):

- First join: 419/419 drawn in 23 to 35 s, about 470 data sources (10 MB), none failed; 591 files under
  `.../distant_horizons/servers/127.0.0.1_25565/<level>_minecraft_overworld/`.
- Horizon band differs by 14.6 on vs off at noon (3.1 at night: there, but dark).
- Second join, same folders: 419/419 from disk in 10 s, 421 up to date, only real-time pushes received.
- In that game: `/dh server off` keeps 419/419 from the same folder (built from chunks, nothing asked);
  `/dh bandwidth 1000` then `/dh server on` starts a new session with the new limit, answered, 419/419 drawn,
  about 400 up to date.
- Folder: with a server list entry "Proof Server", `/dh folder name-ip-port-version` moves LODs to
  `Proof_Server__IP_127.0.0.1-25565__GameVersion_26.3/`, 419/419 drawn; unlisted: `127.0.0.1_25565`. The settings
  page scrolls to all eleven buttons; clicks cycle values.
- By hand: the Nether gets its own folder, 469/469 drawn; rejoin draws 419/419 within 12 s.
- Seen: dark outlines along the near-area edge over water (as the water-edge limit above).

### Paper 26.3 (build 244), Distant Horizons Support

2026-11-07, offline mode, fresh world, plugin 0.15.0-SNAPSHOT (protocol 16) and 0.14.0 (protocol 15).

Reported: "no answer, 0 received". Causes: LZMA2 data flushed but never finished; 0.14 speaks protocol 15 (a
boolean where the session config now has the generator plan); close reasons with an int length.

| Check | Result |
|---|---|
| `cargo test`, clippy, `horizon.py check`, `test` | 29 pass (incl. fixtures from the plugin's own xz library, unfinished); no warnings |
| `interop`, 0.15.0-SNAPSHOT | Pass: configured, 37 sections decoded, none failed |
| `interop`, 0.14.0 | Pass: protocol 15 after "Client is outdated", configured, 37 decoded, none failed, 32 up to date on sync |
| `interop`, Fabric 3.3.4 (again) | Pass: 27 sections, none failed, 26 up to date |
| `server_proof.py --server 127.0.0.1:25567 --limit 900`, 0.15 | Connected, generating; 389/411 drawn after 15 min, about 11,000 data sources (plugin serves only detail level 6, so coarser sections are split), none failed; LODs under `servers/127.0.0.1_25567/world`. The rest waited on the plugin generating the fresh world, so "all drawn" timed out |

**Configuration-phase fix** (reported: "no answer" from an internet server and a LAN server): the first messages
were sent while the connection was still configuring, where Horizon drops them, and never again. Reproduced with
80 ms round trip on loopback (`tc netem`): "no answer", 0 received. Fixed (held until play login, session config
re-sent until answered), same delay: plugin 0.15 connected within 20 s, about 1,200 data sources in 70 s, none
failed; Fabric 3.3.4 419/419 drawn in 40 s. 30 tests pass.

Not tested: a world pregenerated with the plugin's own LOD pregeneration (`builder_type: None`).
