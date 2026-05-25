# Assets

Source content for the game: textures, models, audio, shaders, scenes.

This directory contains **sources** authored by designers/artists, not
runtime data. Sources are processed by the asset pipeline into binary
bundles consumed by the engine at runtime.

Versioned in git with Git LFS for binary files (see `.gitattributes`).

## Layout

- `maps/`       — scene/level definitions (YAML or JSON)
- `textures/`   — texture sources (.psd, .png, .exr)
- `models/`     — 3D model sources (.fbx, .glb)
- `audio/`      — audio sources (.wav, .ogg)
- `shaders/`    — shader sources (.wgsl, .hlsl)
