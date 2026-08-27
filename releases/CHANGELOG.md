# APK Release Archive

Each `*.apk` is a self-contained signed build of the 3D cube chamber. Filenames
are `<UTC-date>_<UTC-time>-<description>.apk`. Use the file size + SHA1 in the
table below to verify which APK is which when you sideload from `:8080`.

| File | Date | Size (bytes) | SHA1 | Notes |
|---|---|---|---|---|
| `2026-08-26_2020-debug-panel-swipe.apk` | 2026-08-26 20:20 UTC | 35988397 | (see `*.sha1`) | Debug effects panel (VFX checkboxes), swipe-to-swap, zoom slider. Branch: `feature/ui-debug-and-swipe`. Tap double-handling fix also included. |
| `2026-08-26_2114-swap-anim-zoom.apk` | 2026-08-26 21:14 UTC | 35988397 | (see `*.sha1`) | **Swap animation** (tiles interpolate positions), **pinch-to-zoom**, improved zoom slider touch targets, selection scale feedback. Tap double-handling fix. |
| `2026-08-27_0748-debug-baseline.apk` | 2026-08-27 07:48 UTC | 36037549 | (see `*.sha1`) | **Baseline match-3/4/5 only** (echo/antipodal/specials OFF by default). Match config addons exposed via FFI. Fixed: gravity-turn over-clearing, pinch-to-zoom, balanced lighting, swipe-turn on any empty area. Swap animation + selection feedback. |