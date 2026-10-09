MarkupCraft for **macOS 11+ on Apple silicon**.

- `markupcraft-*-macos-arm64.dmg`: open it and drag MarkupCraft to Applications. The command-line tool is inside the app at `MarkupCraft.app/Contents/MacOS/markupcraft-cli`.

The build is not notarized by Apple. To open it the first time, right-click the app and choose **Open**
(on macOS 15 and later: try to open it once, then System Settings > Privacy & Security > **Open Anyway**).
Verify downloads with `SHA256SUMS.txt` (`shasum -a 256 -c SHA256SUMS.txt`).
