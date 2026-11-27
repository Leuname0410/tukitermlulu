# tukitermlulu

Open-source SSH/SFTP desktop client inspired by MobaXterm, built with Tauri.

## Install

Download the installer for your platform from the [Releases](https://github.com/Leuname0410/tukitermlulu/releases) page and verify it with the `SHA256SUMS.txt` file published in the same release.

On Linux and macOS:

```sh
sha256sum -c SHA256SUMS.txt
```

On Windows (PowerShell), compare the output with the corresponding line of `SHA256SUMS.txt`:

```powershell
Get-FileHash .\tukitermlulu_*.msi -Algorithm SHA256
Get-FileHash .\tukitermlulu_*-setup.exe -Algorithm SHA256
```

### macOS

Builds are not notarized or signed by Apple. If Gatekeeper blocks the app, right-click it and choose **Open**, or remove the quarantine attribute after moving it to Applications:

```sh
xattr -dr com.apple.quarantine /Applications/tukitermlulu.app
```

### Windows

Builds are not code-signed. When SmartScreen warns about the installer, choose **More info** → **Run anyway**.

### Linux

Make the AppImage executable:

```sh
chmod +x *.AppImage
```

Or install the Debian package:

```sh
sudo apt install ./tukitermlulu_*.deb
```
