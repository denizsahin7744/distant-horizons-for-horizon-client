# Changelog

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
