# App updates — Spec

| | |
| --- | --- |
| Status | Implemented |
| Area | desktop shell, desktop UI, release pipeline |
| Created | 2026-11-10 |
| Updated | 2026-11-11 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Notifications](../notifications/spec.md) (the update notice), [Memory and the Scheduler](../memory-and-scheduler/spec.md) (scheduled tasks across a restart) |

## 1. Summary

evo finds out by itself that a newer release exists, downloads it in the
background, checks that it really comes from the evo project, and installs
it when the person chooses to restart. Today the app only shows its own
version: a person learns about a release by visiting the download page, and
installs it by hand. Releases are frequent, so most people run an old
version without knowing it. The app never restarts on its own, and it never
interrupts a working chat without asking.

## 2. Scope

### 2.1 Goals

- The app learns about a newer release without the person doing anything.
- The update is downloaded and verified in the background; chats keep
  working meanwhile.
- Installing takes one deliberate action from the person, and the app
  reopens on the new version with chats and settings intact.
- Nothing is installed unless it is proven to come from the evo project.
- The person can check by hand, see what the app is doing, and turn
  automatic checks off.
- Every published release can be picked up by the versions already
  installed.

### 2.2 Non-goals

- Release channels (beta, nightly) and choosing a version to install. There
  is one channel: the latest published release.
- Going back to an older version. A newer version may change the saved data
  in ways an older one cannot read.
- Updating anything other than the app and the engine shipped with it:
  plugins, skills, language servers and the optional Office runtime keep
  their own update paths.
- Installing an update on Linux packages, or a macOS release that was not
  signed with the project's Apple developer identity; there the app only
  reports the new version (FR-12).
- Skipping one version so that it is not offered again, and waiting for
  working chats to finish before restarting. Ignoring the notification is
  enough.
- Updates forced by the project (a release the person cannot decline).
- Release notes of the versions between the installed one and the latest:
  the dialog shows the latest release's notes only (FR-16).

## 3. User scenarios

### US-1 Get the new version without looking for it (priority P1)

A person who uses evo every day wants the app to bring them the new release
so that they do not have to watch the download page.

Acceptance scenarios:

1. **Given** an installed version older than the latest release and
   automatic checks on, **when** the app has been running long enough for a
   check, **then** the update is downloaded and verified in the background
   and a notification says that version is ready to install.
2. **Given** an update is ready and no chat is working, **when** the person
   clicks **Restart to update**, **then** the app closes, installs the
   update and reopens on the new version with the same chats and settings.
3. **Given** the app is hidden in the tray, **when** an update becomes
   ready, **then** the person gets an OS notification and nothing is
   installed until they act.

### US-2 Do not lose work to an update (priority P1)

A person with agents working wants an update to wait for them so that no
run is cut off.

Acceptance scenarios:

1. **Given** an update is ready and two chats are working, **when** the
   person clicks **Restart to update**, **then** the app says two chats are
   working and asks whether to restart anyway.
2. **Given** that question, **when** the person cancels, **then** nothing
   changes: the chats keep working and the update stays ready.
3. **Given** an update is ready, **when** the person ignores it, **then**
   the app keeps running the installed version for as long as they like.

### US-3 Check by hand and see the state (priority P2)

A person who heard about a release wants to check right now and see what
the app is doing.

Acceptance scenarios:

1. **Given** **Settings → About**, **when** the person clicks **Check for
   updates**, **then** the row shows the check running and ends on one of:
   up to date, downloading with progress, ready to install, or could not
   check with the reason.
2. **Given** the installed version is the latest, **when** a check
   finishes, **then** the row says the app is up to date and when it was
   last checked.

### US-4 Decide whether the app checks (priority P2)

A person on a restricted network, or one who prefers to update by hand,
wants to turn automatic checks off.

Acceptance scenarios:

1. **Given** **Check for updates** is set to **Manually** in **Settings →
   About**, **when** the app runs for days, **then** it makes no update
   request and downloads nothing.
2. **Given** the setting is off, **when** the person clicks **Check for
   updates** in **Settings → About**, **then** the check runs as in US-3.

### US-5 Learn about a release on a Linux package (priority P3)

A person who installed evo from a Linux package wants to know a new version
exists, so that they can install it with their package manager.

Acceptance scenarios:

1. **Given** evo installed from a Linux package and a newer release,
   **when** a check finds it, **then** a notification names the new version
   and opens the download page; the app downloads and installs nothing.

### US-6 See what changed before updating (priority P2)

A person told about a new version wants to read what it changes, in the
app, before they restart for it.

Acceptance scenarios:

1. **Given** automatic checks are on, **when** a check finds a newer
   version, **then** a dialog opens by itself with the version, its release
   date and its notes grouped as breaking changes, features, fixes,
   performance and other changes, and the action that applies (**Restart to
   update**, or **Download** where the app cannot install).
2. **Given** the person closed that dialog, **when** later checks find the
   same version, or the app restarts, **then** the dialog does not open by
   itself again; **What's new** in **Settings → About** opens it.
3. **Given** the person checks by hand in **Settings → About**, **when** the
   check finds a newer version, **then** the About row shows it and no
   dialog opens by itself.

### Edge cases

- No network, or the release page cannot be reached: an automatic check
  ends silently and is retried at the next check; a check by hand shows the
  reason.
- The download is cut off, or the computer sleeps during it: the installed
  version is untouched and the download is tried again at the next check.
- The downloaded file does not pass verification: it is deleted, the person
  is told the update could not be verified, and nothing is installed.
- A newer release is published while an older update is ready: the newest
  one replaces it; the person is offered only the newest.
- An update is ready and the person quits the app the normal way: nothing
  is installed silently; the update is offered again after the next start.
- Several chat windows are open: **Restart to update** closes all of them
  and the app reopens.
- A scheduled task falls due while the app is restarting: it is handled as
  after any other restart of the app (see
  [Memory and the Scheduler](../memory-and-scheduler/spec.md)).
- The app cannot replace itself where it is installed (for example it runs
  from a read-only place): the install fails with a message that links to
  the download page, and the installed version keeps working.
- A macOS release was built without the project's Apple developer
  identity: macOS apps are told about the version and sent to the download
  page, as on a Linux package (FR-12).
- A development build or the browser preview: the update row says updates
  are not available in this build, and no check runs.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | With automatic checks on, the app checks for a newer release shortly after it starts and then at a regular interval for as long as it runs, including while it is only in the tray. The interval is 6 hours. | Given a running app and a release published after it started, when 6 hours have passed, then the app has found the release. |
| FR-2 | **Settings → About** has **Check for updates**: **Automatically** (the default) or **Manually**. Set to manually, the app makes no automatic check and no download; a check by hand still works. The setting applies without restarting the app. | Given the setting is Manually, when the app runs past one interval, then no update request was made. |
| FR-3 | **Settings → About** shows the installed version, the update state and the time of the last check, with a **Check for updates** button. The states are: up to date, checking, downloading (with progress), ready to install (with the new version), could not check (with the reason), and not available in this build. | Given the About page, when a check by hand runs against a newer release, then the row moves through checking, downloading and ready to install. |
| FR-4 | A release is offered only when its version is higher than the installed one. An equal or lower version is never offered or installed. | Given the latest release equals the installed version, when a check runs, then the state is up to date and nothing is downloaded. |
| FR-5 | A found update is downloaded in the background straight away, without asking the person. Chats, the terminal and the rest of the app keep working during the download. | Given a chat is streaming a reply, when an update downloads, then the reply continues without a pause the person can notice. |
| FR-6 | A downloaded update is installed only if it is proven to come from the evo project: it must match the project's release signature, checked against a key built into the installed app. An update that fails the check is deleted and reported, and the installed version stays as it is. | Given a release file that was altered after it was signed, when the app downloads it, then verification fails, nothing is installed, and the About row says the update could not be verified. |
| FR-7 | When an update is ready, the app raises one notification through the [notifications](../notifications/spec.md) pipeline: it names the new version, opens **Settings → About**, and carries a **Restart to update** action. A later check for the same version does not add a second entry. | Given an update became ready, when the person opens the bell, then there is exactly one entry for that version. |
| FR-8 | The update is installed only when the person chooses **Restart to update** (on the notification or in **Settings → About**). The app never installs or restarts on its own. | Given an update has been ready for 24 hours and the person did nothing, when they look at the app, then it still runs the installed version. |
| FR-9 | If any chat is working when the person chooses **Restart to update**, the app first asks for confirmation and says how many chats are working. Cancelling leaves everything running and the update ready. The app does not offer to wait for the chats to finish; the person chooses **Restart to update** again later. | Given one working chat, when the person chooses **Restart to update** and then cancels, then the chat is still working and the update is still ready. |
| FR-10 | On confirmation the app stops every chat's engine cleanly, installs the update and reopens by itself on the new version. Chats, settings, credentials and connectors are kept; a chat that was working is saved as after any other stop and can be continued. | Given an update is ready, when the person confirms the restart, then the app reopens and **Settings → About** shows the new version with the earlier chats in the sidebar. |
| FR-11 | Wherever the app names a new version (the notification, the About row, the new-version dialog), the person can reach that release's notes: **What's new** in the About row opens the dialog of FR-16, and the dialog links to the release page (or the full comparison with the previous release). | Given an update is ready, when the person clicks **What's new**, then the dialog shows the release's notes. |
| FR-12 | Where the app cannot install an update itself, it downloads and installs nothing: on a Linux package install, and on macOS when the release was not signed with the project's Apple developer identity. A found release raises a notification that names the version and opens the download page, and the About row says a new version is available with the same link. | Given evo installed from a Linux package and a newer release, when a check runs, then nothing is downloaded and the notification opens the download page. |
| FR-13 | A failed automatic check is silent to the person and is recorded in the app's log. A failed check by hand, a failed download and a failed install are shown with the reason and can be retried. After any failure the installed version keeps working. | Given no network, when the person clicks **Check for updates**, then the row says it could not check and why, and the button can be clicked again. |
| FR-14 | Development builds and the browser preview do not check for updates; the About row says updates are not available in this build. | Given a development build, when the app runs, then no update request is made. |
| FR-16 | The new-version dialog shows the version found, the installed version, the release date, and the release's notes laid out by kind (breaking changes, features, fixes, performance, other changes) with a count of each; each change shows its area and links to its commit. Notes in another shape are shown as formatted text; a release without notes links to its release page. The dialog offers the action that applies to the update's state: **Restart to update** when ready, the download progress while downloading, **Download** (the download page) where the app cannot install (FR-12). The notes come with the update information every release publishes (FR-15); nothing else is requested. | Given a release whose notes list two features and one fix, when the dialog opens, then it shows a Features group with two entries, a Fixes group with one, and "2 features · 1 fix". |
| FR-17 | When an automatic check finds a newer version (ready to install, or available on the download page), the dialog of FR-16 opens by itself, once per version: after the person closes it, it does not open by itself for that version again, also after a restart. A check by hand does not open it (the About row shows the result). The notification of FR-7 is raised as before. | Given an automatic check found 4.4.0 and the person closed the dialog, when the app restarts and finds 4.4.0 again, then no dialog opens. |
| FR-15 | Every published release carries what the installed apps need to find it, verify it and install it, for each platform that installs in the app (NFR-1). A release that is missing any of it is not published as the latest release. One exception: a macOS build made without the project's Apple developer identity is published without the macOS part, so those apps get the notice of FR-12. | Given a new release is published, when an app one version behind checks, then it finds, verifies and installs that release. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Updates install in the app on Windows x64 and on macOS (Intel and Apple Silicon) for releases signed with the project's Apple developer identity. Linux packages and other macOS releases are notified only (FR-12). | Manual check: update from the previous release on each of the three platforms. |
| NFR-2 | Update information and files are fetched over an encrypted connection only. The secret half of the release key exists only in the release pipeline's secrets: never in the repository, in a build log or in the app. | Review: the repository and a published app contain only the public half of the key; a release build log shows no key material. |
| NFR-3 | A check sends nothing about the person or the computer beyond what any download of a public file reveals: no account, no identifier, no usage data. | Review of the request the app makes during a check. |
| NFR-4 | On Windows, installing an update needs no administrator rights for an app installed for the current user. | Given a standard user account, when an update installs, then no elevation prompt appears. |
| NFR-5 | Checking and downloading never block the window or delay startup: the first check starts after the window is shown. | Given a slow network, when the app starts, then the window opens as fast as with automatic checks off. |
| NFR-6 | An update that is interrupted at any point (download, verification, install) leaves a working app: either the old version or the new one, never a broken install. | Given the download is cut off halfway, when the app is used and restarted, then it runs the installed version normally. |
| NFR-7 | The app can still be built without the release key (a contributor's machine, a manual pipeline run): the build succeeds and simply cannot be used as an update. | Given no release key, when the app is built, then the build succeeds. |

## 5. Success criteria

- SC-1 A person one or more versions behind reaches the latest release
  without opening the download page, in at most two clicks (**Restart to
  update**, and the confirmation when chats are working).
- SC-2 A running app with automatic checks on offers a new release within
  one day of its publication.
- SC-3 No working chat is ever stopped by an update without the person
  confirming it.
- SC-4 An update that was altered or damaged is never installed.
- SC-5 After an update, every chat and setting the person had is still
  there.

## 6. Clarifications

### 6.1 Open

n/a — no open questions.

### 6.2 Resolved

- 2026-11-10 — Q: does the app install updates itself, or only say that a
  new version exists? → A: it downloads, verifies and installs them itself
  (now in FR-5 to FR-10); a notice only is the fallback where an in-app
  install is not possible (FR-12).
- 2026-11-10 — Q: when a new release is found, does the app download it
  straight away, or ask first? → A: it downloads straight away in the
  background and asks only before installing (now in FR-5, FR-8).
- 2026-11-10 — Q: what do people on a Linux package get? → A: a notice
  with a link to the download page, no in-app install; the package manager
  replaces a system package, not the app (now in FR-12, US-5).
- 2026-11-10 — Q: when chats are working, is a confirmation enough, or
  should there also be "update when the chats finish"? → A: confirmation
  only (now in FR-9, section 2.2).
- 2026-11-10 — Q: how often does a running app check? → A: shortly after
  start and every 6 hours (now in FR-1).
- 2026-11-10 — Q: are published macOS releases always signed and notarized
  with the project's Apple developer identity? → A: not guaranteed, so
  in-app install on macOS is promised for signed releases only; a release
  built without the identity is published without the macOS update
  information and those apps get the notice instead (now in FR-12, FR-15,
  NFR-1).
- 2026-11-10 — Q: can the person skip one version so that it is not
  offered again? → A: no; ignoring the notification is enough and the
  About row keeps showing the update (now in section 2.2).

## 7. History

- 2026-11-10 — Draft.
- 2026-11-10 — The six open questions answered by the owner; no open
  clarification left.
- 2026-11-10 — Gate A: spec accepted by the owner; plan and tasks
  written.
- 2026-11-10 — Gate B: plan accepted by the owner; implementation
  started.
- 2026-11-11 — Owner request: show the release notes in a new-version
  dialog when an automatic check finds a version (US-6, FR-16, FR-17;
  FR-11 updated, the release-notes non-goal narrowed), and choose automatic
  or manual checks in **Settings → About** (FR-2 moved from General).
- 2026-11-10 — Implemented. The owner closed the feature without running
  the remaining checks (end-to-end updates, workflow runs, manual app
  checks); they are marked waived in [tasks.md](tasks.md), and failures
  found later are fixed as bugs.
