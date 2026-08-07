# Publishing to WinGet

After the signed NSIS installer is built, compute its SHA-256 hash in PowerShell:

```powershell
(Get-FileHash .\Handy.Tool_1.0.1_x64-setup.exe -Algorithm SHA256).Hash
```

Replace `REPLACE_WITH_SHA256` in the installer manifest with that uppercase hash. Validate the three manifests with `winget validate --manifest .\winget`, then fork [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs), place them under `manifests/p/patrick-1984/HandyTool/1.0.1/`, and submit a pull request. The `wingetcreate` tool can also create the fork branch and submission from these release assets.
