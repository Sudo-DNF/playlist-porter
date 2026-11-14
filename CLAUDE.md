# playlist-porter

A Rust CLI that lets a user export **their own** Spotify playlists (owned or
collaborative) to CSV/JSON, or recreate them as **private** YouTube playlists.
Personal, non-commercial use only. It moves playlist *metadata* only; it never
touches audio.

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

### Spotify access
- Use only the official Spotify Web API (`https://api.spotify.com/v1`). No
  scraping, headless browsers, bots, private/unofficial endpoints, or web-player
  tokens (Terms IV.2.4.a).
- Auth: **Authorization Code with PKCE**. No client secret. Redirect URI must be
  a loopback IP literal: `http://127.0.0.1:<port>/callback`. Never `localhost`.
- Scopes: exactly `playlist-read-private playlist-read-collaborative`. Do not
  add scopes.

### Which playlists may be processed (Policy III.9)
- Only playlists the signed-in user **owns** (`owner.id == me.id`) or that are
  **collaborative**. This is the policy's exception for moving a user's own
  playlist metadata to another service.
- Since March 2026 the API returns `items` only for these playlists anyway.
  Followed playlists show up in `list`, marked **not exportable**. `export` and
  `to-youtube` refuse them with a clear message.
- No way to process a playlist by arbitrary URL/ID that bypasses the ownership
  check. Accepting the user's own playlist by ID/URL/name is fine; the check
  always runs.

### Content
- **No audio**: no downloading, stream ripping, yt-dlp/youtube-dl, ffmpeg,
  previews or playback of any kind (Terms IV.2.2.c).
- Never mix, sync, play or overlap Spotify content with another service's audio
  (Policy III.5, III.7). The YouTube feature only creates a playlist of YouTube
  video IDs. It never plays anything.
- No analytics, statistics, "insights", listening profiles, recommendations or
  aggregate reports built from Spotify data (Policy III.13). The match report
  is a per-track status list, not analysis.
- No AI/ML: don't send Spotify data to models, embeddings or LLM APIs, and
  don't use it for training (Terms IV.2.1.a, Policy III.14). Matching is
  deterministic string/duration logic only.

### Storage and deletion
- No database or cache of Spotify content beyond what a single run needs
  (Terms IV.3). Hold track data in memory and drop it when the command ends.
- Persistent outputs:
  1. The export file the user explicitly asked for (`export`).
  2. The `to-youtube` resume state (see below). Keep it minimal, delete it
     automatically, and remove it on `logout`.
- OAuth tokens live **only** in the OS keyring (`keyring` crate), service name
  `playlist-porter`, entries `spotify` and `google`. Never write tokens to
  disk, logs, env files or error messages.
- `logout`, run immediately:
  - delete both keyring entries
  - revoke the Google token programmatically (`POST
    https://oauth2.googleapis.com/revoke`), as YouTube policy III.D.2.c
    requires
  - delete all resume state and temp files
  - tell the user that Spotify has no revoke endpoint, so they can remove
    access at https://www.spotify.com/account/apps
  - tell the user that export files they created are theirs and aren't tracked
    or deleted
  
  This satisfies Terms V.8 / Appendix A.5.c (delete within 5 days of
  disconnect) and YouTube III.E.4.g (7 days).

### Attribution (Policy II.4)
- Wherever Spotify metadata is shown or exported, state that it is **supplied
  by Spotify**, and include the Spotify URL of **each track and each playlist**
  (`external_urls.spotify`).
  - `list`: a footer line "Playlist data supplied by Spotify" plus each
    playlist's URL.
  - CSV: the first line is a `#` comment crediting Spotify and giving the
    playlist URL, then the header row. Each row has a `spotify_url` column.
  - JSON: top-level `attribution` and `playlist_url` fields, plus
    `spotify_url` per track.
  - YouTube playlist description: "Track list supplied by Spotify:
    <spotify playlist URL>". The terminal report includes each track's
    Spotify URL.
- Don't use Spotify logos or marks beyond plain-text attribution. Never imply
  endorsement (Policy II.1, VI.2; Terms IX.7).

### Naming
- The product name is **playlist-porter**. Never put "Spotify" in the
  binary, crate or product name, or in the app name registered in the
  Spotify dashboard (Terms IV.2.2.b, Policy VI.2). The local folder name
  doesn't matter, but don't propagate it.

### Commercial
- Personal, non-commercial use only. No ads, telemetry, crash reporting,
  analytics, phone-home, data selling or sharing (Terms IV.2.5, V.5;
  Policy IV.2). The only network hosts contacted are `accounts.spotify.com`,
  `api.spotify.com`, `accounts.google.com`, `oauth2.googleapis.com` and
  `www.googleapis.com`.

### YouTube-specific
- Request only the scope you need: `https://www.googleapis.com/auth/youtube`
  (needed to create playlists and insert items).
- Before creating anything, show the playlist name and track count, say that a
  **private** YouTube playlist will be created, and ask for confirmation
  (`--yes` skips the prompt).
- `--help` and the README must link YouTube's ToS
  (https://www.youtube.com/t/terms) and the Google Privacy Policy
  (http://www.google.com/policies/privacy), say that using the tool means
  agreeing to YouTube's ToS, explain what data is accessed and stored, and
  link https://security.google.com/settings/security/permissions for revoking
  access.

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
  subscription has lapsed." Include Spotify's `error.message` verbatim.
- **Endpoints used** (no others):
  - `GET /v1/me`: current user ID (to check ownership)
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
- Search max `limit=10`. This tool never calls Spotify search.
- **429**: honour the `Retry-After` header (seconds). Retry with backoff up to
  a cap. If the body says `QUOTA_EXCEEDED`, stop with a clear message instead
  of looping.
- **Token refresh**: refresh on 401 or before `expires_at`, persist the new
  token (and the rotated refresh token, if Spotify returns one) to the
  keyring, then retry once.

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

- OAuth: Google "Desktop app" client, Authorization Code + PKCE with a loopback
  redirect `http://127.0.0.1:<port>/callback`. Google desktop clients also
  need the client secret. Load it from config/env, never hard-code it.
- Default quota is **10,000 units/day**, reset at midnight Pacific. Costs:
  - `search.list`: **100**
  - `videos.list` (`contentDetails` for duration, up to 50 IDs per call): **1**
  - `playlists.insert`: **50**
  - `playlistItems.insert`: **50**
  
  That's roughly 150 units per track, so about **65 tracks/day**. Document this
  prominently.
- Track estimated units used in-process. Before each call, check there's
  enough budget left; on a `quotaExceeded` 403, stop cleanly.
- **Resume**: write `$XDG_STATE_HOME/playlist-porter/resume/<spotify_playlist_id>.json`
  (via the `dirs` crate) holding only: Spotify playlist ID, YouTube playlist
  ID, index of the next track, and the per-track results so far (YouTube video
  ID + status). Don't store Spotify track metadata. On resume, re-fetch the
  playlist from Spotify. Delete the file when the run finishes, on `logout`,
  and automatically when it is older than 7 days (well under YouTube's 30-day
  limit).
  
  `to-youtube <playlist> --resume` continues. Without `--resume`, an existing
  state file prompts the user to resume or start over.

### Matching rules
Keep these in a pure, side-effect-free module (`src/matching.rs`) so they can
be unit-tested:

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
  - **matched**: best score above a high threshold *and* within ±5 s
  - **uncertain**: added, but below the high threshold or outside ±5 s
  - **not found**: nothing usable; not added
- The final report lists every track with its status, YouTube URL (if any) and
  Spotify URL. Counts per status are fine; no other statistics.

---

## Commands

| Command | Behaviour |
|---|---|
| `login` | Spotify PKCE in the browser (prints the URL too), stores the token in the keyring. |
| `list` | User's playlists: name, track count (if exportable), owner, **exportable / not exportable**, Spotify URL, attribution footer. |
| `export <playlist> --format csv\|json [-o FILE]` | Columns/fields: `title, artists, album, duration_ms (and mm:ss), isrc (optional), spotify_url`, with an attribution header. Default filename derived from the playlist name; never overwrite without `--force`. |
| `to-youtube <playlist> [--resume] [--yes]` | Google login if needed, confirmation, private playlist, match + insert, report, quota-aware stop/resume. |
| `logout` | Deletes both keyring entries, revokes the Google token, deletes resume/temp data, prints the Spotify revoke link. |

`<playlist>` accepts a playlist ID, a `https://open.spotify.com/playlist/…`
URL, a `spotify:playlist:…` URI, or an exact name from `list`. Ownership is
checked every time.

The **disclaimer** (below) appears in `--help` (clap `after_help` on the
top-level command) and in the README.

---

## Technical

- Rust, edition 2021+. Crates: `tokio`, `reqwest` (rustls), `serde`,
  `serde_json`, `clap` (derive), `csv`, `oauth2`, `keyring`, `thiserror`
  (library errors), `anyhow` (in `main`). Small helpers allowed: `dirs`,
  `toml`, `open` (launch the browser), `url`. Ask before adding anything else.
- Config: env vars override `~/.config/playlist-porter/config.toml`:
  - `PLAYLIST_PORTER_SPOTIFY_CLIENT_ID`
  - `PLAYLIST_PORTER_GOOGLE_CLIENT_ID`
  - `PLAYLIST_PORTER_GOOGLE_CLIENT_SECRET`
  - `PLAYLIST_PORTER_REDIRECT_PORT` (default `8888`)
  
  Never hard-code or commit client IDs/secrets. Keep `config.toml` and `.env`
  in `.gitignore`. Ship a `config.example.toml` with placeholders.
- Suggested layout: `src/main.rs` (clap), `src/config.rs`, `src/auth/`
  (`spotify.rs`, `google.rs`, `keyring.rs`, loopback server), `src/spotify/`
  (`client.rs`, `models.rs`), `src/youtube/` (`client.rs`, `quota.rs`,
  `resume.rs`), `src/matching.rs`, `src/export.rs`, `src/error.rs`.
- Tests (`cargo test`, no network):
  - matching: live/cover/remix exclusion, ±5 s preference, feat./remaster
    normalization, status thresholds, ISO-8601 duration parsing
  - Spotify models: fixtures in `tests/fixtures/` covering the new `items`/`item`
    shape, responses with both old and new names, absent `items` for followed
    playlists, `null` items, episodes, local files, missing `isrc`
  - pagination via `next`, plus a 429 `Retry-After` / `QUOTA_EXCEEDED` parser
  - CSV/JSON output includes attribution and per-track Spotify URLs
- Before finishing a change, run `cargo fmt`, `cargo clippy -- -D warnings`
  and `cargo test`.
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
   - Config/env vars.
2. Usage examples for every command.
3. Limitations:
   - own/collaborative playlists only
   - 5-user Dev Mode cap and the Premium requirement
   - ~65 tracks/day YouTube quota, and how resume works
   - imperfect matching
   - local files and episodes are skipped
4. Privacy: what's stored (keyring tokens, temporary resume file), how to
   delete it (`logout`), the revoke links, YouTube ToS and Google Privacy
   Policy links.
5. The disclaimer.

## Disclaimer (verbatim, in README and `--help`)

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

- Remote: `origin` → https://github.com/cybergatovgm/playlist-porter
- Default branch: `main`
- **After every commit, always push to the remote immediately:**

  ```bash
  git push origin HEAD
  ```

  On the first push of a new branch, use `git push -u origin HEAD`.
- If a push is rejected (the remote has new commits), run
  `git pull --rebase origin <branch>`, resolve any conflicts, and push again.
  Never force-push without asking first.
