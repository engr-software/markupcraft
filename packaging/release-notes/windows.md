MarkupCraft for **Windows 10/11 (x64)**.

- `markupcraft-*-windows-x64-setup.exe`: installer (Start menu entry, "Open with" for PDFs, uninstaller in Apps & features).
- `markupcraft-*-windows-x64.zip`: portable; unzip it anywhere and run `markupcraft.exe`. `markupcraft-cli.exe` is the command-line tool.

The build is not code-signed, so Windows SmartScreen may show a prompt the first time: choose **More info**, then **Run anyway**.
Verify downloads with `SHA256SUMS.txt` (`certutil -hashfile <file> SHA256`).
