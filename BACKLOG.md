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
