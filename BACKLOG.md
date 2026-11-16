# Backlog — deferred ideas

Things to revisit later, not currently scheduled. Active work is tracked in
`CLAUDE.md` (Status), `docs/handoff.md`, and `docs/decisions.md`.

## Custom GitHub Pages domain (cleaner URL)

- **Context (2026-11-15):** the app is live as a GitHub Pages *project site* at
  `https://tanhminh.github.io/painting-prep/` — the address is derived from the
  owner (`tanhminh.github.io`) plus the repo name (`/painting-prep/`), not
  chosen. A **custom domain** would give a cleaner address, e.g.
  `paintingprep.<your-domain>` or an apex like `paintingprep.app`.
- **How, when we do it:** Settings → Pages → *Custom domain*. GitHub writes a
  `CNAME` file into the repo and provisions an HTTPS certificate. Then add DNS:
  - subdomain → a `CNAME` record pointing at `tanhminh.github.io`, or
  - apex/root domain → GitHub Pages' `A`/`AAAA` records.
  Needs a domain you own (registration cost).
- **Caveat — base path:** a custom domain usually serves from the **root** (`/`)
  instead of the `/painting-prep/` subpath. The app's relative asset paths
  (`./pkg/…`, `style.css`) keep working, but the **service worker** (M4
  Milestone C) registration scope and its precached URL list are base-path
  sensitive — so change the domain either before adding the service worker, or
  revisit the service worker's scope/cache list at the same time.
- **Status:** requested 2026-11-15, revisit later. No domain chosen yet.

## Cross-platform export (Android / desktop) as a first-class feature

- **Context (2026-11-16):** the tester (Linh, feedback #17) used the app on
  Android/Windows and found the export flow confusing — the "Tap Share → Save
  Image" copy is iOS share-sheet wording, and on platforms where
  `navigator.canShare({files})` is false the Share button hides, leaving only the
  `<a download>` "Save" with inconsistent styling. The app is **iPhone-first by
  scope** (spec §2), not by technical limit: the Rust→wasm core and the rest of
  the UI already run in any modern browser.
- **Idea, when we do it:** make the export step **platform-aware** — detect
  `navigator.canShare`/`share` support, show copy that matches the platform
  (iOS: Share → Save to Photos; elsewhere: a plain Download), and give Share/Save
  one consistent button style. This revises the two-step export (spec F4 / D26),
  so log it as a decision.
- **Note:** the *general* export cleanup (consistent styling, clearing the
  Share/Save buttons when settings change) is being done now in the UI/UX pass
  (plan theme T7); only the **platform-specific** behaviour is deferred here.
- **Status:** deferred 2026-11-16 during the UI/UX polish pass.
