<p align="center">
  <img alt="MarkupCraft" src="assets/logo.svg" width="128">
</p>

<h1 align="center">MarkupCraft</h1>

<p align="center">
  <b>PDF markup and quantity takeoff; an open-source, clean-room app that reads and writes Revu-compatible markups, written in Rust.</b><br>
  Mark up drawings, set scales, measure lengths, areas and counts, and hand the same PDF back to anyone on the job.<br>
  Windows · macOS · Linux · the web
</p>

<p align="center">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-12a58a">
  <img alt="Written in Rust" src="https://img.shields.io/badge/written%20in-Rust-b7410e">
  <img alt="Platforms: Windows, macOS, Linux, web" src="https://img.shields.io/badge/runs%20on-Windows%20%C2%B7%20macOS%20%C2%B7%20Linux%20%C2%B7%20web-2f7bf5">
  <img alt="Status: early alpha" src="https://img.shields.io/badge/status-early%20alpha-d69e2e">
  <img alt="No account, no telemetry" src="https://img.shields.io/badge/no%20account-no%20telemetry-0a7563">
</p>

<p align="center">
  <a href="#highlights">Highlights</a> ·
  <a href="#screenshots">Screenshots</a> ·
  <a href="#features">Features</a> ·
  <a href="#compatibility-scorecard">Scorecard</a> ·
  <a href="#install">Install</a> ·
  <a href="#build-from-source">Build</a> ·
  <a href="#command-line">CLI</a> ·
  <a href="#how-its-built">How it's built</a> ·
  <a href="#license">License</a>
</p>

---

## Highlights

<table>
<tr>
<td width="33%" valign="top">

### Compatible
Markups and measurements are saved as standard PDF annotations with the keys Revu reads and
writes, so a set marked up in MarkupCraft opens in Revu and the other way round.

</td>
<td width="33%" valign="top">

### Measured
Every quantity is computed from geometry and the page scale, then checked against the label
Revu itself saved, on a real 121-sheet marked-up set. See the [scorecard](#compatibility-scorecard).

</td>
<td width="33%" valign="top">

### Yours
No account, no telemetry, no cloud. Works offline. The engine, the command-line tool and the
app are all open source.

</td>
</tr>
</table>

## Screenshots

> Screenshots arrive with the first desktop release. They will live in `docs/images/` and show the
> canvas with a marked-up drawing, the Markups List with totals, and the measurement tools.

## Features

MarkupCraft is in early alpha. The file layer and the measurement engine are working and tested
against real Revu-made files; the desktop interface is being built on top of them. Progress per
feature is tracked honestly in `parity/revu-features.toml` (`cargo xtask parity`).

**Measurements**
- Length, polylength, area, perimeter, volume and count, with page scales (`/Measure` and
  `/VP` viewports) read and written the way Revu stores them.
- Imperial and metric units, fractional and decimal display, captions placed where Revu places them.
- Quantities and their label text match Revu's on every measurement in the reference set.

**Markups**
- Lines, arrows, polylines, polygons, rectangles, ellipses, clouds, text boxes, callouts,
  highlights and stamps, with colors, line styles, line endings and opacity.
- Saved with appearance streams, so every PDF viewer shows them, not just MarkupCraft.

**Takeoff**
- A Markups List with subjects, units, counts and totals, exportable to CSV.
- Custom columns stored with the document, so the file is the project.

**Safe with your files**
- Saves write a temporary file and then rename it, so a crash never leaves half a PDF.
- Every PDF is untrusted input: damaged files report an error, never crash the app.

**Automation**
- Everything the app does is a call into a headless engine, also reachable from
  `markupcraft-cli` and, when you switch it on, a local MCP server for agents (loopback only,
  token-protected, off by default).

## Compatibility scorecard

The scorecard is the project's proof of compatibility. It runs `markupcraft-cli` over a real
set of drawings marked up in Revu (not included in this repository) and counts:

| Step | What it checks | Target |
|---|---|---|
| `check` | our quantity for every measurement equals the label Revu saved | 410/410 |
| `resave` | every measurement survives a full rewrite and reload with the same quantity | 410/410 |
| `markupcheck` | every other markup survives a rewrite and reload unchanged (geometry, text, colors, style) | 187/187 |

```sh
MARKUPCRAFT_REF_PDF=/path/to/marked-up-set.pdf cargo xtask scorecard
```

Without `MARKUPCRAFT_REF_PDF` the scorecard is skipped. Thresholds can be changed with
`--check N --resave N --markupcheck N` (or `MARKUPCRAFT_SCORE_CHECK`, `_RESAVE`, `_MARKUPCHECK`).
How behavior is proven beyond the scorecard is described in [docs/EQUIVALENCE.md](docs/EQUIVALENCE.md).

## Install

Downloads are on the [Releases page](https://github.com/engr-software/markupcraft/releases).
Each version has one release per operating system (`vX.Y.Z-windows`, `vX.Y.Z-macos`,
`vX.Y.Z-linux`), each with a `SHA256SUMS.txt`.

### Windows 10/11 (x64)

- **Installer:** `markupcraft-<version>-windows-x64-setup.exe` adds a Start menu entry, an
  "Open with" entry for PDFs, and an uninstaller.
- **Portable:** unzip `markupcraft-<version>-windows-x64.zip` anywhere and run `markupcraft.exe`.

The builds are not code-signed yet. If SmartScreen shows a prompt, choose **More info**, then **Run anyway**.

### macOS 11+ (Apple silicon)

Open `markupcraft-<version>-macos-arm64.dmg` and drag MarkupCraft to Applications. The builds are
not notarized yet: the first time, right-click the app and choose **Open** (on macOS 15 and later,
use System Settings > Privacy & Security > **Open Anyway**).

### Linux (x86_64)

- **AppImage:** `chmod +x markupcraft-<version>-linux-x86_64.AppImage` and run it (needs FUSE 2,
  or run it with `--appimage-extract-and-run`).
- **tar.gz:** unpack it and run `bin/markupcraft`; the `.desktop` entry and icons are under `share/`.

Needs glibc 2.35 or newer and the usual desktop libraries (GTK 3, Wayland or X11, Vulkan or OpenGL).

## Build from source

You need Rust 1.95 or newer ([rustup.rs](https://rustup.rs)). On Linux, also the windowing and
GTK development packages:

```sh
# Debian / Ubuntu
sudo apt install libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxrandr-dev \
  libxi-dev libgl1-mesa-dev libgtk-3-dev
```

```sh
git clone https://github.com/engr-software/markupcraft
cd markupcraft
cargo run --release -p markupcraft          # the desktop app
cargo run --release -p markupcraft-cli -- list drawing.pdf
cargo xtask ci                              # fmt, clippy, tests, wasm check, parity
cargo xtask package                         # this OS's packages into dist/
```

Project tasks run through `cargo xtask` (`cargo xtask help` lists them). Releases are described
in [docs/releasing.md](docs/releasing.md).

## Command line

`markupcraft-cli` uses the same engine as the app:

```text
markupcraft-cli list <pdf> [--csv out.csv]       markups and totals per subject
markupcraft-cli check <pdf>                      our quantities vs the label Revu saved
markupcraft-cli resave <in.pdf> <out.pdf>        rewrite every measurement, reload, compare
markupcraft-cli markupcheck <in.pdf> <out.pdf>   rewrite every other markup, reload, compare
markupcraft-cli demo <in.pdf> <out.pdf>          add one of each measurement to page 1
```

It exits 0 when everything matched, 1 when something differs or fails, and 2 on a usage error.

## How it's built

MarkupCraft follows the conventions of the Crafting Apps family ([PdfCraft](https://github.com/storytold/pdfcraft),
PhotoCraft): a Rust workspace with `apps/`, `crates/` and `xtask/`, an egui + wgpu interface on
top of a headless engine, and a never-crash rule for every input. It builds on PdfCraft's PDF
object layer and page renderer. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
[docs/ARTCRAFT_FORMAT.md](docs/ARTCRAFT_FORMAT.md).

```
apps/      markupcraft (desktop), markupcraft-cli, markupcraft-web (WASM)
crates/    geom, measure, model, revu, render, engine, automation, ui-egui
xtask/     ci, scorecard, parity, package
packaging/ per-OS package scripts and release notes
```

## Clean-room and trademarks

MarkupCraft is an independent project, written from public documentation (ISO 32000 and Revu's
public help), PDFs we own read with a PDF parser, and black-box observation. It is not affiliated
with, endorsed by or sponsored by Bluebeam, Inc. Bluebeam and Revu are trademarks of their
respective owners and are used here only to describe file compatibility. No Bluebeam code, icons,
artwork, fonts or sample files are used.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT)
at your option. Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in MarkupCraft, as defined in the Apache-2.0 license, shall be dual licensed as
above, without any additional terms or conditions.

Third-party components and assets are listed in [THIRD_PARTY.md](THIRD_PARTY.md).
