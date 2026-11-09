# Running the app yourself

How to build and serve the Painting Prep PWA so you can open it on the iPhone.
Run everything from the repo root (`painting-prep/`).

## One-time setup (already done on this machine)
- `cargo install wasm-bindgen-cli --version 0.2.129 --locked`
  (must match the `wasm-bindgen` pin in `Cargo.lock`; `build-web.sh` refuses to run otherwise)
- If `cargo`/`wasm-bindgen` are "not found": `export PATH="$HOME/.cargo/bin:$PATH"`

## Every time

```bash
# 1. Build the wasm into web/pkg/
./scripts/build-web.sh

# 2. Serve web/ on the LAN
python3 -m http.server 8000 --bind 0.0.0.0 -d web
```

Leave the server running in that terminal; Ctrl-C stops it.

## Open on the iPhone
1. Find the laptop's LAN IP (the `enp0s8` adapter, **not** `10.0.2.15`, which is NAT):
   ```bash
   ip -4 addr show enp0s8 | grep -oP 'inet \K[\d.]+'
   ```
   The IP is DHCP-assigned and **changes between sessions** — check it each time.
   (As of 2026-11-08 it was `10.0.0.143`; earlier it was `10.0.0.157`.)
2. On the iPhone, open `http://<that-ip>:8000`.

## Notes
- Plain HTTP on the LAN → no service worker, no Web Share (that's M4, over HTTPS/Pages).
- Safari caches `app.js`. After a rebuild, **close and reopen the Safari tab** to pick up changes.
- A quick local check that the server is up:
  ```bash
  curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8000/
  ```
