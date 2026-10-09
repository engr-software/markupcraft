MarkupCraft for **Linux x86_64** (glibc 2.35 or newer: Ubuntu 22.04, Debian 12, Fedora 36 and later).

- `markupcraft-*-linux-x86_64.AppImage`: `chmod +x` it and run it. It needs FUSE 2
  (`sudo apt install libfuse2t64` on Ubuntu 24.04), or run it with `--appimage-extract-and-run`.
- `markupcraft-*-linux-x86_64.tar.gz`: one folder holding `bin/markupcraft`, `bin/markupcraft-cli`, and the
  `.desktop` entry and icons under `share/`; run `bin/markupcraft` from it, or copy `bin/` and `share/` into `~/.local`.

The app uses the system's GTK 3 (file dialogs), Wayland or X11, and Vulkan or OpenGL drivers.
Verify downloads with `SHA256SUMS.txt` (`sha256sum -c SHA256SUMS.txt`).
