# Publishing to WinGet

The three manifests in this folder are the exact set submitted to
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) for the current release. Keep
them in step with the published installer — a mismatched hash is the most common cause of a
rejected submission.

## Submitting a new version

1. Build and publish the release so the installer is downloadable from a permanent URL. The
   manifest must point at a versioned release asset, never a `latest` alias.

2. Hash the **published** asset, not the local build output. They should be identical, but the
   validator downloads from the URL, so that is the one that must match:

   ```powershell
   $u = 'https://github.com/patrick-1984/handy-tool/releases/download/v1.1.0/Handy.Tool_1.1.0_x64-setup.exe'
   Invoke-WebRequest $u -OutFile handy.exe -UseBasicParsing
   (Get-FileHash handy.exe -Algorithm SHA256).Hash
   ```

3. Update `PackageVersion` in all three files, and `InstallerUrl`, `InstallerSha256` and
   `ReleaseDate` in the installer manifest. Refresh `ReleaseNotes` and `ReleaseNotesUrl` in the
   locale manifest.

4. Validate locally:

   ```powershell
   winget validate --manifest .\winget
   ```

5. Fork the repository, place the files under
   `manifests/p/patrick-1984/HandyTool/<version>/`, and open a pull request containing **only**
   those three files. A PR that also touches documentation or tooling fails the first validation
   step.

`wingetcreate` can automate steps 2–5.

## Things that have actually gone wrong here

**A missing Visual C++ runtime.** The validator reported exit `-1073741515` on launch, which
unsigned is `0xC0000135` — `STATUS_DLL_NOT_FOUND`. The application depended on the VC++
redistributable, which is absent from a clean image. It is now deployed app-locally beside
`handy.exe`, so there is no runtime prerequisite and no `Dependencies` entry is needed. If that
ever regresses, the validator will catch it again. Confirm before submitting by extracting the
NSIS payload and checking for `msvcp140.dll`, `msvcp140_1.dll`, `msvcp140_atomic_wait.dll`,
`vcruntime140.dll`, `vcruntime140_1.dll` and `concrt140.dll`.

**A stale schema.** Submissions were originally made on manifest schema 1.6.0 with the minimum
viable field set. Current practice is 1.12.0 with a `# yaml-language-server: $schema=` header on
every file and full metadata. Richer manifests give a reviewer less to ask about.

**Smart App Control on the build host.** When Smart App Control is enforced, it blocks the
unsigned installer from running inside Windows Sandbox, so the local clean-image test cannot
complete. This is a property of the host, not the package — verified by running the same test
against a build that had previously passed it and finding it blocked identically. See
[`../winget-tests/`](../winget-tests/) for the harness and results.

## Testing before submission

[`../winget-tests/`](../winget-tests/) contains the validation harness: a preflight that verifies
the published hash and extracts the NSIS payload, and a Windows Sandbox test that installs
silently and asserts the application actually starts with its runtime modules loaded.
