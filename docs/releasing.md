# Releasing MarkupCraft

A pushed version tag `vX.Y.Z` runs `.github/workflows/release.yml`. It checks that the tag
matches the version in `Cargo.toml`, builds the packages for every OS (the same
`.github/workflows/package.yml` that CI runs on every push), and publishes **three separate
GitHub releases**:

| Release | Files |
|---|---|
| `vX.Y.Z-windows` | `markupcraft-X.Y.Z-windows-x64-setup.exe`, `markupcraft-X.Y.Z-windows-x64.zip`, `SHA256SUMS.txt` |
| `vX.Y.Z-macos` | `markupcraft-X.Y.Z-macos-arm64.dmg`, `SHA256SUMS.txt` |
| `vX.Y.Z-linux` | `markupcraft-X.Y.Z-linux-x86_64.AppImage`, `markupcraft-X.Y.Z-linux-x86_64.tar.gz`, `SHA256SUMS.txt` |

Each release's notes are `packaging/release-notes/<os>.md`. The per-OS tags that the releases
create (`vX.Y.Z-windows` and so on) do not match the workflow's tag filter, so they do not
start another run.

## Cutting a release

1. **Bump the version.** It lives only in `[workspace.package] version` in the root
   `Cargo.toml`. Change it, run `cargo check` so `Cargo.lock` follows, and commit both:

   ```sh
   cargo xtask version          # prints the current version
   ```

2. **Make sure `main` is green.** CI (`.github/workflows/ci.yml`) must pass on the commit you
   tag: fmt, clippy, tests on all three OSes, the wasm check, and the package jobs. Run the
   scorecard locally against the reference set too:

   ```sh
   MARKUPCRAFT_REF_PDF=/path/to/marked-up-set.pdf cargo xtask scorecard
   ```

3. **Review the release notes** in `packaging/release-notes/`. They describe the files and how
   to install them; edit them when the set of files or the requirements change.

4. **Tag and push:**

   ```sh
   git tag v0.3.0
   git push origin v0.3.0
   ```

5. **Check the releases.** When the workflow finishes, the Releases page has
   `v0.3.0-windows`, `v0.3.0-macos` and `v0.3.0-linux`. Download one file per OS and compare it
   with `SHA256SUMS.txt`.

If the tag and `Cargo.toml` disagree, the first job fails and nothing is built: delete the tag
(`git push --delete origin v0.3.0`), fix the version, and tag again. Re-running a failed workflow
for the same tag replaces the assets of any release it already created.

## Building packages locally

`cargo xtask package` builds this OS's packages into `dist/` by running the script in
`packaging/` (`--skip-build` reuses the release binaries already built):

| Host | Script | Needs |
|---|---|---|
| Windows | `packaging/windows/package.ps1` | Rust (MSVC); NSIS 3 for the installer (without it only the zip is built) |
| macOS | `packaging/macos/package.sh` | Rust, Xcode command line tools (`codesign`, `hdiutil`, `iconutil`, `sips`) |
| Linux | `packaging/linux/package.sh` | Rust, the GTK/X11/Wayland dev packages; `appimagetool` is downloaded when missing |

The version comes from `Cargo.toml`; set `MARKUPCRAFT_VERSION` to override it for a test build,
and `DIST` to write somewhere other than `dist/`.

## Signing

The packages are not code-signed by a certificate yet:

- **Windows:** the executables and installer are unsigned; SmartScreen asks once (**More info**,
  **Run anyway**).
- **macOS:** the app and disk image are ad-hoc signed, which Apple silicon requires to run, but
  not notarized. `MACOS_SIGN_IDENTITY` makes `package.sh` sign with a real Developer ID instead;
  notarization would be the next step.
- **Linux:** AppImages and tarballs are not signed; `SHA256SUMS.txt` is the integrity check.

The release notes carry the one-line instructions for opening an unsigned app on each OS.
