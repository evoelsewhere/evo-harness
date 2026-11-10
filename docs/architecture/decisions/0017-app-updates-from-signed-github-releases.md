# 0017. The app updates itself from signed GitHub releases

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-10 |
| ADD sections | [2. System context](../README.md#2-system-context), [4. Process view](../README.md#4-process-view), [7. Data view](../README.md#7-data-view), [10. Deployment view](../README.md#10-deployment-view), [12. Risks](../README.md#12-risks-and-technical-debt) |
| Related | [App updates spec](../../features/app-updates/spec.md), [plan](../../features/app-updates/plan.md) |

## Context

People install evo from the project's public GitHub releases and replace it
by hand. Releases come every few days, so most installed copies are old.
The [app updates spec](../../features/app-updates/spec.md) asks the app to
find, download, verify and install new releases itself, on Windows and
signed macOS builds, and to only announce them on Linux packages.

Installing a downloaded program is a new trust boundary: until now nothing
the app downloads is ever run as the app. The release pipeline already
builds every platform from a `v*` tag and publishes a GitHub release. Tauri
offers an updater plugin that verifies a minisign signature against a
public key compiled into the app; the check cannot be turned off.

## Decision

The desktop shell updates the app with `tauri-plugin-updater`, from a
static manifest (`latest.json`) published with every GitHub release, and
installs only files signed with the project's release key.

- **One key pair.** The public key is in the app's configuration. The
  private key is only in the release pipeline's secrets and the owner's
  offline backup. Tag builds without it fail.
- **The shell owns updates.** It checks, downloads, verifies (again just
  before installing) and installs from Rust. The webview only shows the
  state and passes the person's clicks; it gets no updater permission, and
  the agent has no tool for it.
- **Fixed source, no downgrade.** The endpoint is fixed at build time;
  release builds offer only a higher version than the running one.
- **The person decides when.** The app never installs or restarts on its
  own; it asks before stopping working chats.
- **Where the app cannot replace itself, it only announces.** Linux
  packages and macOS builds without the project's Apple identity get no
  update entry in the manifest; those apps link to the download page.

## Consequences

- Updates reach people without a visit to the download page; the release
  pipeline must publish a complete `latest.json` before a release becomes
  the latest.
- **The key is a single point of failure.** If it is lost, installed apps
  can no longer be updated in the app; if it leaks, anyone who can also
  serve the manifest could push an install. Either way the recovery is a
  release with a new public key that people install by hand. The owner
  keeps an offline backup and rotates by that route only.
- Versions up to 4.2.1 have no updater: the first release with it is
  installed by hand.
- A new external source of code (GitHub Releases), new app settings
  (`updates.*`) and a cache folder for downloads, all in the ADD.
- The shell depends on `tauri-plugin-updater` behaviour (installer
  arguments on Windows, bundle swap on macOS, the order in which it
  compares versions and looks up platforms); every plugin upgrade is
  checked against the update flow before release.
- Contributors and agents: never commit a private key, never add a way to
  change the update endpoint or key in release builds, never expose update
  commands to the agent or to chat windows.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Only announce new versions on every platform | Keeps the manual install that the spec removes. |
| A custom updater (own download, verification, installer launch) | Repeats the plugin's per-platform install logic; more code on a security boundary. |
| A custom update server | A service to run and secure for a single release channel. |
| Webview-driven updates through the plugin's JS API | Any window or script could trigger an install; checks would depend on the webview being loaded. |
| Trust HTTPS and GitHub without a signature | A compromised account, release or CDN would install directly on every machine. |
