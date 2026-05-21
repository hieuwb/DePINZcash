# Deploying DePINZcash

Backend on **Fly.io**, frontend on **Vercel**, domain **zcashdepin.com**.
Run these in order. Each step is one-time unless noted.

---

## 0. Prerequisites

```bash
# install once
curl -L https://fly.io/install.sh | sh    # flyctl
npm i -g vercel                            # vercel CLI

# auth (browser flow)
flyctl auth login
vercel login
```

Generate an admin key — keep this somewhere safe, you'll need it to publish reward snapshots:

```bash
openssl rand -hex 32
```

---

## 1. Backend → Fly.io

From `server/`:

```bash
cd server

# Create the app (skip if 'depinzcash-server' already exists).
# --no-deploy because we need to create the volume first.
flyctl apps create depinzcash-server

# Persistent volume for the SQLite database.
flyctl volumes create depinzcash_data --region iad --size 1 --yes

# Secrets — these are encrypted at rest on Fly.
flyctl secrets set \
  ADMIN_API_KEY="$(openssl rand -hex 32)" \
  TRUSTED_RPCS="" \
  SPL_MINT=""

# Deploy. --remote-only uses Fly's builders so you don't need local Docker.
flyctl deploy --remote-only

# Verify
flyctl status
curl https://depinzcash-server.fly.dev/healthz
curl https://depinzcash-server.fly.dev/api/info | jq
```

Add the API subdomain:

```bash
flyctl certs add api.zcashdepin.com
# Outputs DNS records to add — see DNS section below.
```

When you later have real Zcash JSON-RPC endpoints, update `TRUSTED_RPCS`:

```bash
flyctl secrets set TRUSTED_RPCS="https://rpc1.example,https://rpc2.example,https://rpc3.example"
```

---

## 2. Frontend → Vercel

From `web/`:

```bash
cd web

# Link this directory to a new Vercel project.
vercel link

# Set the build-time env vars (these get baked into the static bundle).
vercel env add VITE_API_URL production
# paste: https://api.zcashdepin.com

vercel env add VITE_ZCASH_NETWORK production
# paste: mainnet

vercel env add VITE_SOLANA_CLUSTER production
# paste: devnet     (later: mainnet-beta once $ZePIN is on Solana mainnet)

# Deploy.
vercel --prod
# returns a https://depinzcash.vercel.app (or similar) URL
```

Attach the domain in the Vercel dashboard or via CLI:

```bash
vercel domains add zcashdepin.com
vercel domains add www.zcashdepin.com
```

---

## 3. DNS — Cloudflare / Namecheap / wherever zcashdepin.com is registered

You'll need three records. The exact targets come from `flyctl certs show api.zcashdepin.com` and Vercel's domain UI; pattern is:

| Host | Type | Value | Provider |
|---|---|---|---|
| `api` | CNAME | `depinzcash-server.fly.dev.` | Fly |
| `@` (apex) | A | `76.76.21.21` | Vercel |
| `www` | CNAME | `cname.vercel-dns.com.` | Vercel |

After records propagate (usually <5 min):

```bash
flyctl certs check api.zcashdepin.com    # status: issued
curl https://api.zcashdepin.com/healthz
curl https://zcashdepin.com
```

Then update the backend's CORS list to include the live origins (already set in `fly.toml`, but if you change domains):

```bash
flyctl secrets set CORS_ALLOWED_ORIGINS="https://zcashdepin.com,https://www.zcashdepin.com"
```

---

## 4. End-to-end smoke

```bash
# api up?
curl -s https://api.zcashdepin.com/healthz
# {"status":"ok"}

# info reports the right network + mint?
curl -s https://api.zcashdepin.com/api/info | jq

# website loads + CORS works?
curl -s -H "Origin: https://zcashdepin.com" -I https://api.zcashdepin.com/api/info \
  | grep -i access-control-allow-origin
# access-control-allow-origin: https://zcashdepin.com
```

Open `https://zcashdepin.com`, connect Phantom on `/register`, sign — should round-trip.

---

## Updating

- **Backend** code change → `cd server && flyctl deploy --remote-only`
- **Frontend** code change → push to GitHub (Vercel auto-deploys main), or `vercel --prod`
- **Secret** rotation → `flyctl secrets set KEY=value` (triggers a restart)
- **Volume** size bump → `flyctl volumes extend <id> --size <gb>`

---

## Rollback

```bash
flyctl releases list
flyctl releases rollback <version>
# or
vercel rollback <deployment-url>
```

---

## Common breakages

- **`flyctl deploy` fails on volume** — make sure `depinzcash_data` exists in the same region as the app (`iad`).
- **CORS error in browser** — `CORS_ALLOWED_ORIGINS` must match the exact origin including scheme + port. No trailing slash.
- **Cert stuck "pending"** — DNS hasn't propagated; `flyctl certs check api.zcashdepin.com` and wait.
- **Phantom signing returns nothing** — wallet adapter version mismatch; verify `@solana/wallet-adapter-react-ui` is loaded (Network tab) and Phantom isn't blocked by an ad blocker.
- **`/api/wallet/<addr>/claim/latest` is 404** — no snapshot has been published yet. Hit `POST /api/admin/snapshot/publish` with `x-admin-key`.
