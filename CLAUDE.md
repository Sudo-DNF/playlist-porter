# playlist-porter

A Rust desktop app with an **egui** GUI. It lets a user export **their own**
Spotify playlists (owned or collaborative) to CSV/JSON, or recreate them as
**private** YouTube playlists. The same binary also has a small CLI for
scripting. Personal, non-commercial use only. It moves playlist *metadata*
only; it never touches audio.

The project must comply with:

- Spotify Developer Terms v10 (effective 15 May 2025): https://developer.spotify.com/terms
- Spotify Developer Policy (effective 15 May 2025): https://developer.spotify.com/policy/
- Spotify February 2026 Dev Mode migration guide: https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide
- YouTube API Services Terms and Developer Policies: https://developers.google.com/youtube/terms/developer-policies

If a requested change would break any rule below, **stop and say so** instead
of implementing it. Re-read the sources above before changing anything
compliance-related; they change.

---

## Compliance rules (non-negotiable)

These apply equally to the GUI and the CLI.

### Spotify access
- Use only the official Spotify Web API (`https://api.spotify.com/v1`). No
  scraping, headless browsers, embedded web views of spotify.com, bots,
  private/unofficial endpoints, or web-player tokens (Terms IV.2.4.a).
- Auth: **Authorization Code with PKCE** in the user's **system browser**
  (never an embedded web view). No client secret. Redirect URI must be a
  loopback IP literal: `http://127.0.0.1:<port>/callback`. Never `localhost`.
- Scopes: exactly `playlist-read-private playlist-read-collaborative`. Do not
  add scopes.

### Which playlists may be processed (Policy III.9)
- Only playlists the signed-in user **owns** (`owner.id == me.id`) or that are
  **collaborative**. This is the policy's exception for moving a user's own
  playlist metadata to another service.
- Since March 2026 the API returns `items` only for these playlists anyway.
  Followed playlists still appear in the playlist list, marked **Not
  exportable**. Their Export and YouTube actions are disabled, with a tooltip
  explaining why. The CLI refuses them with a clear message.
- No way to process a playlist by arbitrary URL/ID that bypasses the ownership
  check: no "paste any playlist URL" box in the GUI. The CLI may accept the
  user's own playlist by ID/URL/name, and the ownership check always runs.

### Content
- **No audio**: no downloading, stream ripping, yt-dlp/youtube-dl, ffmpeg,
  previews or playback of any kind (Terms IV.2.2.c). The GUI never embeds a
  player.
- Never mix, sync, play or overlap Spotify content with another service's audio
  (Policy III.5, III.7). The YouTube feature only creates a playlist of YouTube
  video IDs. It never plays anything. Showing YouTube links that open in the
  system browser is fine.
- No analytics, statistics, charts, "insights", listening profiles,
  recommendations or aggregate reports built from Spotify data (Policy
  III.13). The match report is a per-track status list, not analysis.
- No AI/ML: don't send Spotify data to models, embeddings or LLM APIs, and
  don't use it for training (Terms IV.2.1.a, Policy III.14). Matching is
  deterministic string/duration logic only.

### Storage and deletion
- No database or cache of Spotify content beyond what the running app needs
  (Terms IV.3). Hold playlist/track data in memory only and drop it on sign-out
  and on exit. Re-fetch when the user clicks Refresh; don't cache it.
- **eframe persistence**: if enabled, it may save only UI prefs (window size,
  theme, last export format/folder). Never save Spotify or YouTube data, tokens
  or playlist selections in it. Prefer leaving eframe's `persistence` feature
  off and storing UI prefs in `config.toml`.
- Don't cache cover-art images to disk. If they're shown, load them into
  memory only. The simplest option is not to show them.
- Persistent outputs:
  1. The export file the user explicitly asked for.
  2. The YouTube transfer resume state (see below). Keep it minimal, delete it
     automatically, and remove it on sign-out.
- OAuth tokens live **only** in the OS keyring (`keyring` crate), service name
  `playlist-porter`, entries `spotify` and `google`. Never write tokens to
  disk, logs, env files, the config file or error messages.
- **Sign out / Disconnect** (GUI Settings and the `logout` CLI command), run
  immediately:
  - delete both keyring entries
  - revoke the Google token programmatically (`POST
    https://oauth2.googleapis.com/revoke`), as YouTube policy III.D.2.c
    requires
  - delete all resume state and temp files
  - clear all in-memory Spotify/YouTube data
  - tell the user that Spotify has no revoke endpoint, so they can remove
    access at https://www.spotify.com/account/apps (shown as a clickable link)
  - tell the user that export files they created are theirs and aren't tracked
    or deleted
  
  Offer separate "Disconnect Spotify" and "Disconnect YouTube" buttons plus a
  "Sign out of everything" button. This satisfies Terms V.8 / Appendix A.5.c
  (delete within 5 days of disconnect) and YouTube III.E.4.g (7 days).

### Attribution (Policy II.4)
- Wherever Spotify metadata is shown or exported, state that it is **supplied
  by Spotify**, and link the Spotify URL of **each track and each playlist**
  (`external_urls.spotify`).
  - GUI: a persistent footer on any screen showing Spotify data reading
    "Playlist and track data supplied by Spotify." Every playlist row and track
    row has an "Open in Spotify" link (`ui.hyperlink_to`), opened in the system
    browser.
  - CSV: the first line is a `#` comment crediting Spotify and giving the
    playlist URL, then the header row. Each row has a `spotify_url` column.
  - JSON: top-level `attribution` and `playlist_url` fields, plus
    `spotify_url` per track.
  - YouTube playlist description: "Track list supplied by Spotify:
    <spotify playlist URL>". The match report (GUI table and CSV/JSON report
    export) includes each track's Spotify URL.
- Don't use Spotify logos, the Spotify green or any marks beyond plain-text
  attribution. The app icon and branding must not resemble Spotify's. Never
  imply endorsement (Policy II.1, VI.2; Terms IX.7).

### Naming
- The product name is **playlist-porter**. Never put "Spotify" in the
  binary, crate, window title, app ID or product name, or in the app name
  registered in the Spotify dashboard (Terms IV.2.2.b, Policy VI.2). The local
  folder name doesn't matter, but don't propagate it.

### Commercial
- Personal, non-commercial use only. No ads, telemetry, crash reporting,
  update checks, analytics, phone-home, data selling or sharing (Terms
  IV.2.5, V.5; Policy IV.2). The only network hosts contacted are
  `accounts.spotify.com`, `api.spotify.com`, `accounts.google.com`,
  `oauth2.googleapis.com` and `www.googleapis.com`.

### YouTube-specific
- Request only the scope you need: `https://www.googleapis.com/auth/youtube`
  (needed to create playlists and insert items).
- Before creating anything, show a **confirmation modal** with the playlist
  name, track count, estimated quota/days needed, and a clear note that a
  **private** YouTube playlist will be created on the user's account. In the
  CLI this is a prompt; `--yes` skips it.
- Before the first Google sign-in, and on the About/Privacy screen, the GUI
  must:
  - link YouTube's ToS (https://www.youtube.com/t/terms) and the Google
    Privacy Policy (http://www.google.com/policies/privacy)
  - say that using the YouTube feature means agreeing to YouTube's ToS
  - explain what data is accessed and stored
  - link https://security.google.com/settings/security/permissions for
    revoking access
  
  The same text goes in the CLI `--help` and the README.

---

## Spotify Web API notes (post-Feb/March 2026)

Many tutorials, crates and LLM memories are outdated. Trust these:

- **Dev Mode**: the app owner needs an **active Spotify Premium** subscription.
  If it lapses, the app stops working. Max **5 authorized users**, added by
  hand in the dashboard. `GET /me` no longer returns `product`, so Premium
  can't be checked up front.
- **Errors**: Spotify doesn't document the exact error for a lapsed Premium or
  an unlisted user. Map any 403 on an otherwise valid call to a clear message:
  "Spotify refused access. Either your account isn't in this app's user list
  (Developer Dashboard → User Management), or the app owner's Premium
  subscription has lapsed." Include Spotify's `error.message` verbatim. In the
  GUI, show it in an error panel with a link to the README's setup section.
- **Endpoints used** (no others):
  - `GET /v1/me`: current user ID and display name (to check ownership)
  - `GET /v1/me/playlists?limit=50`: paginate with `next`
  - `GET /v1/playlists/{id}`: metadata, owner, collaborative,
    `external_urls`
  - `GET /v1/playlists/{id}/items?limit=50`: paginate with `next`
- **Renamed fields**: in playlist objects `tracks` → `items`. In paging
  objects the playlist-item entries hold the track under `item` (formerly
  `track`). The API may currently send **both** old and new names. Deserialize
  with `#[serde(alias = "tracks")]` / `#[serde(alias = "track")]` and prefer
  the new name.
- `items` is **absent** for playlists the user doesn't own or collaborate on.
  Model it as `Option`.
- An entry's `item` can be `null` (removed/unavailable track), an episode
  (`type: "episode"`), or a local file (`is_local: true`, no Spotify ID or
  URL). Skip and report these. Don't crash.
- **Removed**: `popularity`, `available_markets`, `linked_from` on tracks.
  Batch endpoints (`GET /tracks?ids=`, `/albums`, `/artists`) are gone. Don't
  use them; the track object inside playlist items has everything needed.
- `external_ids.isrc` was removed and then restored. Treat it as
  `Option<String>`.
- Search max `limit=10`. This app never calls Spotify search.
- **429**: honour the `Retry-After` header (seconds). Retry with backoff up to
  a cap, and show "Rate limited, retrying in N s" in the GUI. If the body says
  `QUOTA_EXCEEDED`, stop with a clear message instead of looping.
- **Token refresh**: refresh on 401 or before `expires_at`, persist the new
  token (and the rotated refresh token, if Spotify returns one) to the
  keyring, then retry once. If refresh fails, return the GUI to the sign-in
  screen.

### Why not `rspotify`
`rspotify` 0.16.x (Mar/Apr 2026) did migrate to `/items` and accepts both
field names. We still call the API directly with `reqwest` and our own serde
models because:
- it caches tokens in a JSON file by default (we require keyring-only),
- we need only four endpoints,
- we want full control over 403/429 handling and the field aliases.

Don't add `rspotify` without discussing it first.

---

## YouTube Data API v3 notes

- OAuth: Google "Desktop app" client, Authorization Code + PKCE in the system
  browser with a loopback redirect `http://127.0.0.1:<port>/callback`. Google
  desktop clients also need the client secret. Load it from config/env, never
  hard-code it.
- Default quota is **10,000 units/day**, reset at midnight Pacific. Costs:
  - `search.list`: **100**
  - `videos.list` (`contentDetails` for duration, up to 50 IDs per call): **1**
  - `playlists.insert`: **50**
  - `playlistItems.insert`: **50**
  
  That's roughly 150 units per track, so about **65 tracks/day**. Show the
  estimate in the confirmation modal and in the README.
- Track estimated units used in-process and show them in the transfer view
  (e.g. "Quota used today ≈ 4,350 / 10,000"; this is YouTube quota, not
  Spotify analytics). Before each call, check there's enough budget left. On a
  `quotaExceeded` 403, stop cleanly, save resume state, and tell the user when
  quota resets.
- **Resume**: write `$XDG_STATE_HOME/playlist-porter/resume/<spotify_playlist_id>.json`
  (via the `dirs` crate) holding only: Spotify playlist ID, YouTube playlist
  ID, index of the next track, and the per-track results so far (YouTube video
  ID + status). Don't store Spotify track metadata. On resume, re-fetch the
  playlist from Spotify. Delete the file when the run finishes, on sign-out,
  and automatically when it is older than 7 days (well under YouTube's 30-day
  limit).
  - GUI: a playlist with saved state shows a **Resume transfer** button
    beside **Start over**.
  - CLI: `to-youtube <playlist> --resume`.
- The user can **Cancel** a running transfer. Cancelling stops after the
  current API call and saves resume state.

### Matching rules
Keep these in a pure, side-effect-free module (`src/core/matching.rs`) so they
can be unit-tested. Never put them in UI code.

- Query: `"<primary artist> <title>"`. Strip `- Remastered 20xx`, `(feat. …)`
  and similar suffixes from the query, but keep them for comparison.
- Fetch candidates with `search.list` (`type=video`, `maxResults=10`), then one
  `videos.list` call for durations (ISO-8601 `PT#M#S`; parse it yourself).
- Reject candidates whose title contains `live`, `cover` or `remix` (whole
  word, case-insensitive) unless the Spotify title also contains that word.
- Score on normalized title similarity, artist presence (in title or channel
  name; boost `"<artist> - Topic"` and VEVO channels) and duration:
  - within **±5 s** of the Spotify `duration_ms` is strongly preferred
  - beyond ±5 s is penalized but not disqualified
- Status per track:
  - **Matched**: best score above a high threshold *and* within ±5 s
  - **Uncertain**: added, but below the high threshold or outside ±5 s
  - **Not found**: nothing usable; not added
- The report lists every track with its status, YouTube link (if any) and
  Spotify link, plus counts per status. No other statistics.
  - GUI: a filterable table (All / Matched / Uncertain / Not found), with a
    "Save report…" button that writes CSV/JSON with attribution.
  - CLI: printed to stdout.

---

## GUI (egui / eframe)

### Screens
1. **Setup**: shown when client IDs are missing. Fields for the Spotify client
   ID and the Google client ID/secret, the redirect URI to register (with a
   copy button), and links to the README setup steps. Save writes to
   `config.toml`; the Google secret is also allowed there. These are app
   credentials, not user tokens.
2. **Sign in**: a "Connect Spotify" button opens the system browser. While
   waiting, show a spinner and a Cancel button, plus the auth URL as
   selectable text in case the browser didn't open. Shows the Dev Mode
   notes: 5-user cap and owner Premium.
3. **Playlists** (main screen): a table (`egui_extras::TableBuilder`) with
   these columns:
   - Name
   - Owner
   - Tracks
   - Status (**Exportable** / **Not exportable** with tooltip)
   - Open in Spotify (link)
   - actions: **Export…**, **Send to YouTube…**
   
   Add a search/filter box over the *already loaded list* (no Spotify search),
   a "Show only exportable" toggle, a Refresh button, and the attribution
   footer. Disabled actions for non-exportable rows.
4. **Playlist detail** (optional): track table with Title, Artists, Album,
   Duration, ISRC, and Open in Spotify, plus the attribution footer.
5. **Export dialog**: format (CSV/JSON) and a save location via a native
   file dialog (`rfd`). Ask before overwriting an existing file. Show a success
   toast with "Show in folder".
6. **YouTube transfer**: the YouTube ToS/privacy notice, then Google sign-in
   (first time), then the confirmation modal, then the progress view:
   - progress bar, current track, quota used, Cancel
   - a live report table
   - when done: summary counts, a link to open the new private playlist, and
     "Save report…"
7. **Settings**: account status for each service, Disconnect Spotify,
   Disconnect YouTube, Sign out of everything, redirect port, theme.
8. **About / Privacy**: the disclaimer (verbatim), what's stored and where,
   revoke links, YouTube ToS and Google Privacy Policy links, and a version
   number.

### Architecture rules
- **Never block the UI thread.** All network, OAuth and file I/O runs on a
  tokio runtime owned by the app (multi-thread runtime, created in `main` and
  kept alive). The UI sends `Command`s to workers through an
  `mpsc`/`tokio::sync` channel. Workers send `Event`s back through a channel
  and call `ctx.request_repaint()`. `update()` drains events each frame with
  `try_recv`.
- UI state is a plain struct with no business logic. Keep everything in
  `src/core/` usable without egui, so the CLI and tests reuse it.
- Long operations report progress events and support cancellation via
  `tokio_util::sync::CancellationToken` (or an `AtomicBool`).
- Errors become user-facing messages in an error panel/toast with an
  actionable hint. No panics on network/API errors.
- The OAuth loopback listener binds `127.0.0.1` only, accepts exactly one
  callback, validates `state`, shows a "You can close this tab" page, and shuts
  down. It times out after 5 minutes.
- Open external links with `ctx.open_url` / `ui.hyperlink_to`, never in an
  embedded view.
- Use egui's built-in light/dark themes, following the system by default. Keep
  layouts readable at 800×600 minimum, and don't rely on colour alone for
  status (use text + icon).

### CLI (same binary)
- Running `playlist-porter` with no subcommand launches the GUI. Subcommands
  are thin wrappers over `src/core/`:
  - `login`
  - `list`
  - `export <playlist> --format csv|json [-o FILE] [--force]`
  - `to-youtube <playlist> [--resume] [--yes]`
  - `logout`
- `<playlist>` accepts a playlist ID, a `https://open.spotify.com/playlist/…`
  URL, a `spotify:playlist:…` URI, or an exact name from `list`. Ownership is
  checked every time.
- The disclaimer appears in `--help` (clap `after_help` on the top-level
  command).

---

## Technical

- Rust, edition 2021+. Crates:
  - GUI: `eframe`/`egui`, `egui_extras` (tables), `rfd` (native file dialogs)
  - core: `tokio`, `tokio-util`, `reqwest` (rustls), `serde`, `serde_json`,
    `csv`, `oauth2`, `keyring`, `thiserror` (in core), `anyhow` (in
    `main`/UI)
  - CLI: `clap` (derive)
  - small helpers: `dirs`, `toml`, `open` (launch browser from the CLI),
    `url`
  
  Ask before adding anything else.
- Pin `egui`, `eframe` and `egui_extras` to the **same version**. egui has
  breaking changes between minor versions, so check the current API docs
  instead of relying on memory.
- On Linux, `keyring` needs a Secret Service provider (GNOME Keyring /
  KWallet). Show a clear error if none is available, and never fall back to
  plaintext.
- Config: env vars override `~/.config/playlist-porter/config.toml` (which the
  Setup screen writes):
  - `PLAYLIST_PORTER_SPOTIFY_CLIENT_ID`
  - `PLAYLIST_PORTER_GOOGLE_CLIENT_ID`
  - `PLAYLIST_PORTER_GOOGLE_CLIENT_SECRET`
  - `PLAYLIST_PORTER_REDIRECT_PORT` (default `8888`)
  
  Never hard-code or commit client IDs/secrets. Keep `config.toml` and `.env`
  in `.gitignore`. Ship a `config.example.toml` with placeholders.
- Suggested layout:
  ```
  src/main.rs            # parse args: no subcommand → GUI, else CLI
  src/cli.rs             # clap definitions + handlers
  src/gui/               # app.rs (eframe::App), screens/*.rs, widgets.rs, worker.rs (Command/Event bridge)
  src/core/config.rs
  src/core/auth/         # spotify.rs, google.rs, keyring.rs, loopback.rs
  src/core/spotify/      # client.rs, models.rs
  src/core/youtube/      # client.rs, quota.rs, resume.rs
  src/core/matching.rs
  src/core/export.rs
  src/core/error.rs
  ```
- Tests (`cargo test`, no network, no GUI):
  - matching: live/cover/remix exclusion, ±5 s preference, feat./remaster
    normalization, status thresholds, ISO-8601 duration parsing
  - Spotify models: fixtures in `tests/fixtures/` covering the new `items`/`item`
    shape, responses with both old and new names, absent `items` for followed
    playlists, `null` items, episodes, local files, missing `isrc`
  - pagination via `next`, plus a 429 `Retry-After` / `QUOTA_EXCEEDED` parser
  - CSV/JSON output includes attribution and per-track Spotify URLs
  - exportability logic (owner/collaborative)
  - resume-state expiry
- Before finishing a change, run `cargo fmt`, `cargo clippy -- -D warnings`
  and `cargo test`. For UI changes, also run the app and check the affected
  screen.
- Keep error messages actionable. Never log tokens, auth codes or full
  response bodies containing tokens.

## README must cover
1. Setup:
   - Spotify developer app: owner needs **Premium**, add up to **5 users**
     under User Management, register redirect URI
     `http://127.0.0.1:8888/callback`, don't use "Spotify" in the app name.
   - Google Cloud project with **YouTube Data API v3** enabled, OAuth consent
     screen (testing mode, add yourself as a test user), "Desktop app" OAuth
     client.
   - Entering the IDs on the Setup screen, or using config/env vars.
   - Linux keyring requirement.
2. Usage: GUI walkthrough (with screenshots once they exist) and CLI examples
   for every subcommand.
3. Limitations:
   - own/collaborative playlists only
   - 5-user Dev Mode cap and the Premium requirement
   - ~65 tracks/day YouTube quota, and how resume works
   - imperfect matching
   - local files and episodes are skipped
4. Privacy: what's stored (keyring tokens, config with client IDs, temporary
   resume file), how to delete it (Sign out / `logout`), the revoke links,
   YouTube ToS and Google Privacy Policy links.
5. The disclaimer.

## Disclaimer (verbatim, in README, About screen and CLI `--help`)

> playlist-porter is an independent project. It is not affiliated with,
> endorsed by, or sponsored by Spotify or Google/YouTube. Track metadata is
> supplied by Spotify. This tool exports only playlist metadata from your own
> account through official APIs and never downloads audio. You are solely
> responsible for how you use the exported data and for following Spotify's
> and YouTube's terms and copyright law where you live. The authors are not
> liable for any misuse, including downloading or sharing copyrighted
> material.

---

## Git workflow

- Remote: `origin` → https://github.com/Sudo-DNF/playlist-porter (private)
- Default branch: `main`
- **After every commit, always push to the remote immediately:**

  ```bash
  git push origin HEAD
  ```

  On the first push of a new branch, use `git push -u origin HEAD`.
- If a push is rejected (the remote has new commits), run
  `git pull --rebase origin <branch>`, resolve any conflicts, and push again.
  Never force-push without asking first.
- The network here sometimes fails with TLS "bad record MAC" errors. Retry the
  push a few times before treating it as a real failure.
