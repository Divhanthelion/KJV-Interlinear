# Releasing KJV Interlinear

Builds for every platform run in GitHub Actions (`.github/workflows/release.yml`) when a tag like `v0.2.0` is pushed. The workflow already builds unsigned apps; each account below turns on signing and store delivery for its platform once its secrets are added under **Settings → Secrets and variables → Actions**.

App identifier on every platform: `io.github.divhanthelion.kjvinterlinear`. It is permanent once any store has published the app.

## What only the owner can do

These need your identity, payment, or tax details, so they can't be automated.

### 1. Apple Developer Program (iPhone, iPad, Mac App Store, notarized Mac downloads)

- Enroll at https://developer.apple.com/programs/ ($99/year). Individual enrollment takes about a day.
- In App Store Connect, create an app with bundle ID `io.github.divhanthelion.kjvinterlinear`, category **Reference**, age rating 4+.
- Secrets the workflow uses today (macOS signing and notarization): `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` (an app-specific password), `APPLE_TEAM_ID`. iOS signing and App Store upload are not wired into the workflow yet; when they are, create an **App Store Connect API key** (Users and Access → Integrations) for automated uploads.
- Before the first iOS upload: commit the generated `app/gen/apple` project with `app/PrivacyInfo.xcprivacy` copied into the bundle (Apple rejects binaries without a privacy manifest, ITMS-91053), and answer the export-compliance question (the app's HTTPS uses rustls, not only the OS libraries — answer the questionnaire accordingly or add `ITSAppUsesNonExemptEncryption` once reviewed).

### 2. Google Play Console (Android)

- Register at https://play.google.com/console ($25 once, identity verification).
- **New personal accounts must run a closed test with at least 12 testers for 14 continuous days before the app can go to production.** Line up testers early.
- Create an upload key (`keytool -genkeypair -v -keystore upload.jks -keyalg RSA -keysize 2048 -validity 10000 -alias upload`), keep it safe, and enroll in Play App Signing.
- Secrets: `ANDROID_KEY_BASE64` (the .jks, base64-encoded), `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`.
- The Data safety form: the app itself collects and shares no data. If the build ships the study assistant, declare the optional, user-initiated sending of questions and attached passages to the user's own AI provider (“Other user-generated content”, optional, encrypted in transit, user-controlled) — check Play's current definitions when filling it in.

### 3. Windows signing and the Microsoft Store

- Without code signing, Windows SmartScreen warns "Windows protected your PC" on the installer from GitHub.
- **Azure Trusted Signing** (about $10/month; individuals in the US and Canada can validate identity) can sign the installers; the signing step is not wired into `release.yml` yet.
- **Microsoft Store**: an individual Partner Center account is free. The app is submitted as an **MSIX** ("MSIX or PWA" in Partner Center), which the Store signs itself, so no certificate is needed for Store copies.
  - The package identity (name, publisher, publisher display name) is in `app/windows/msix/AppxManifest.xml` and must match Partner Center > Product management > Product identity exactly. Update it if Partner Center's values change (for example once account verification replaces the "Applicant" placeholder).
  - Every release build produces the package as the `microsoft-store` workflow artifact (`KJV-Interlinear_<version>.0_x64.msix`); upload it under Packages in the submission. To build one locally: `powershell -File app/windows/msix/pack.ps1 -Exe target/release/kjv-interlinear.exe` (needs the Windows SDK).
  - Restricted capability `runFullTrust` (every desktop app has it). Justification for certification: "A desktop application (Rust with the Microsoft Edge WebView2 runtime) packaged as MSIX; runFullTrust is required for a Win32 desktop app."

### 4. Flathub (Linux)

- Free. Submission is a pull request to https://github.com/flathub/flathub adding a manifest for `io.github.divhanthelion.kjvinterlinear`; Flathub verifies the ID against this GitHub account.

## Release mechanics

- The tag must match the version in `app/tauri.conf.json` and `app/Cargo.toml`; the workflow refuses to build otherwise.
- Unsigned macOS downloads show Gatekeeper's “damaged or incomplete” warning on Apple Silicon until notarization is set up; the Mac App Store build (sandboxed, `.pkg`) is a separate effort not in the pipeline yet.
- Desktop installers from GitHub have no auto-updater; store builds update through their stores.

## Store listing checklist

- Privacy policy URL: https://github.com/Divhanthelion/KJV-Interlinear/blob/main/PRIVACY.md
- App icon: 1024×1024 PNG, no transparency (`app/icons/icon-source.png`; the iOS set under `app/icons/ios/` is exported without an alpha channel, as the App Store requires)
- Screenshots: iPhone 6.9" (1320×2868), iPad 13" (2064×2752), Android phone (1080×1920 or larger), Play feature graphic (1024×500), Mac and Windows desktop
- Content rights: KJV text public domain (outside the UK Crown patent); Hebrew/Greek data CC BY 4.0 from STEP Bible; credited in-app under Settings → About
- Apple guideline 4.3 (spam) often catches Bible apps. Lead the description with what is distinctive: word-by-word Hebrew and Greek following the Textus Receptus, Strong's lexicon, fully offline.
