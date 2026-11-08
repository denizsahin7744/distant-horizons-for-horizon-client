# Changelog

## 0.1.1 (2026-11-08)

- Builds on Horizon `6475f62` (Nightly 2026-11-04) with horizon-mixins `5a64af9`, the commit Mod Menu 0.3.3 and
  other registry mods pin: on it `Remote::commands` and `Game::merge_remote_tree` are gone, which failed every
  player's native build that had this mod in it (#2).
- `/dh` reaches the chat's command tree through the `native_commands` hook (API 35) instead of editing the tree the
  server sent; the chat box still colours and completes `/dh`, no longer its words. Based on #1 by marshaaaal.

## 0.1.0 (2026-10-30)

First release, experimental.

- Distant Horizons' level-of-detail core ported to Rust: section positions and full data points (same bit layouts),
  chunk-to-LOD building, 2 x 2 merging up a quadtree to 16,384-block sections, hidden-run culling, distance-based
  detail with the five horizontal-quality presets, render columns with cave culling, column-box meshing with
  neighbour-aware side faces and face merging.
- Distant generation from Horizon's world generator in single player and on LAN hosts.
- LODs kept on disk per world and dimension.
- Drawn by Horizon's far-terrain renderer with block textures and biome tints; the game's own chunks cut out.
- Settings page (Options > Mods) and `/dh` commands, completed by Tab in chat; a debug view colouring each
  detail level.
- Builds together with Horizon's official native mods; WebAssembly mods run beside it.
- A release zip with an installer for macOS and Linux (`install.command`, `install.sh`) that builds it through
  the Horizon launcher.
- Pinned to Horizon `d009ebd` (Nightly 2026-10-29) and horizon-mixins `0d6b103`.
