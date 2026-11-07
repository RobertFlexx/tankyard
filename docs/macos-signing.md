# Signing and notarizing the macOS installer
# for helpers/maintainers.

Without these steps the release `.pkg` is unsigned, and macOS blocks it when it is downloaded in a browser ("Apple could not verify… is free of malware"). With them, the release workflow signs Tank Yard.app and the package with your Developer ID, has Apple notarize the package, and staples the result. The visual installer then opens with a double-click and no warning.

You need a paid [Apple Developer Program](https://developer.apple.com/programs/) membership.

1. In Xcode → Settings → Accounts → Manage Certificates (or at developer.apple.com → Certificates), create a **Developer ID Application** and a **Developer ID Installer** certificate.
2. In Keychain Access, select both certificates with their private keys, choose **File → Export Items**, and save one `.p12` file with a strong password.
3. At [appleid.apple.com](https://appleid.apple.com) → Sign-In and Security → App-Specific Passwords, create a password for notarization.
4. Add these repository secrets (GitHub → Settings → Secrets and variables → Actions):

| Secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE` | the `.p12` file, base64 encoded: `base64 -i certificates.p12 \| pbcopy` |
| `MACOS_CERTIFICATE_PASSWORD` | the `.p12` export password |
| `APPLE_ID` | your Apple Account email |
| `APPLE_TEAM_ID` | the 10-character team ID shown at developer.apple.com → Membership |
| `APPLE_APP_PASSWORD` | the app-specific password |

The next release signs and notarizes automatically; the build fails rather than publishing a package Apple rejected. Without the secrets, releases stay unsigned and players use `install-macos.sh`.

To sign locally, keep both certificates in your login keychain and run:

```sh
TANKYARD_SIGN_APP="Developer ID Application: Your Name (TEAMID)" \
TANKYARD_SIGN_INSTALLER="Developer ID Installer: Your Name (TEAMID)" \
APPLE_ID=you@example.com APPLE_TEAM_ID=TEAMID APPLE_APP_PASSWORD=xxxx-xxxx-xxxx-xxxx \
python3 scripts/build_macos_installer.py --version v0.10.0
```
