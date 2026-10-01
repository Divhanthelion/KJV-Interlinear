# Releasing KJV Interlinear

Builds for every platform run in GitHub Actions (`.github/workflows/release.yml`) when a tag like `v0.2.0` is pushed. The workflow already builds unsigned apps; each account below turns on signing and store delivery for its platform once its secrets are added under **Settings → Secrets and variables → Actions**.

App identifier on every platform: `io.github.divhanthelion.kjvinterlinear`. It is permanent once any store has published the app.

## What only the owner can do

These need your identity, payment, or tax details, so they can't be automated.

### 1. Apple Developer Program (iPhone, iPad, Mac App Store, notarized Mac downloads)

- Enroll at https://developer.apple.com/programs/ ($99/year). Individual enrollment takes about a day.
- In App Store Connect, create an app with bundle ID `io.github.divhanthelion.kjvinterlinear`, category **Reference**, age rating 4+.
- Create an **App Store Connect API key** (Users and Access → Integrations) for automated uploads.
- Secrets the workflow uses: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` (an app-specific password), `APPLE_TEAM_ID`, plus the API key.

### 2. Google Play Console (Android)

- Register at https://play.google.com/console ($25 once, identity verification).
- **New personal accounts must run a closed test with at least 12 testers for 14 continuous days before the app can go to production.** Line up testers early.
- Create an upload key (`keytool -genkeypair -v -keystore upload.jks -keyalg RSA -keysize 2048 -validity 10000 -alias upload`), keep it safe, and enroll in Play App Signing.
- Secrets: `ANDROID_KEY_BASE64` (the .jks, base64-encoded), `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`.
- The Data safety form: the app collects and shares no data.

### 3. Windows signing and the Microsoft Store

- Without code signing, Windows SmartScreen warns "Windows protected your PC" on the installer from GitHub.
- **Azure Trusted Signing** (about $10/month; individuals in the US and Canada can validate identity) signs the installers in CI.
- **Microsoft Store**: an individual Partner Center account is free. The app is submitted as an **MSIX** ("MSIX or PWA" in Partner Center), which the Store signs itself, so no certificate is needed for Store copies.
  - The package identity (name, publisher, publisher display name) is in `app/windows/msix/AppxManifest.xml` and must match Partner Center > Product management > Product identity exactly. Update it if Partner Center's values change (for example once account verification replaces the "Applicant" placeholder).
  - Every release build produces the package as the `microsoft-store` workflow artifact (`KJV-Interlinear_<version>.0_x64.msix`); upload it under Packages in the submission. To build one locally: `powershell -File app/windows/msix/pack.ps1 -Exe target/release/kjv-interlinear.exe` (needs the Windows SDK).
  - Restricted capability `runFullTrust` (every desktop app has it). Justification for certification: "A desktop application (Rust with the Microsoft Edge WebView2 runtime) packaged as MSIX; runFullTrust is required for a Win32 desktop app."

### 4. Flathub (Linux)

- Free. Submission is a pull request to https://github.com/flathub/flathub adding a manifest for `io.github.divhanthelion.kjvinterlinear`; Flathub verifies the ID against this GitHub account.

## Store listing checklist

- Privacy policy URL: https://github.com/Divhanthelion/KJV-Interlinear/blob/main/PRIVACY.md
- App icon: 1024×1024 PNG, no transparency (pending; see the placeholder in `icon.png`)
- Screenshots: iPhone 6.9" (1320×2868), iPad 13" (2064×2752), Android phone (1080×1920 or larger), Play feature graphic (1024×500), Mac and Windows desktop
- Content rights: KJV text public domain (outside the UK Crown patent); Hebrew/Greek data CC BY 4.0 from STEP Bible; credited in-app under Settings → About
- Apple guideline 4.3 (spam) often catches Bible apps. Lead the description with what is distinctive: word-by-word Hebrew and Greek following the Textus Receptus, Strong's lexicon, fully offline.
