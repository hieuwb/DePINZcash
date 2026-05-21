import { useState } from "react";
import { Link } from "react-router-dom";

type InstallMethod = "source" | "docker";

export function RunLightwalletd() {
  const [method, setMethod] = useState<InstallMethod>("source");

  return (
    <div className="flex flex-col gap-10">
      <section className="grid gap-6 md:grid-cols-2 md:items-center">
        <div className="flex flex-col gap-4">
          <div className="flex flex-wrap items-center gap-2">
            <span className="pill">Operator guide · ~20 min setup</span>
            <span className="inline-flex items-center gap-1 rounded-full border border-zcash-gold/40 bg-zcash-gold/10 px-2 py-0.5 text-[10px] uppercase tracking-wider text-zcash-gold">
              Recommended for newcomers
            </span>
          </div>
          <h1 className="text-3xl font-semibold leading-tight md:text-4xl">
            Run a <span className="text-zcash-gold">lightwalletd</span> server.
          </h1>
          <p className="max-w-prose text-sm text-zcash-subtle">
            Lightwalletd sits in front of a Zcash full node and serves compact-block data
            to light wallets like Ywallet and Nighthawk. Smaller disk than a full node,
            still rewarded by DePINZcash — the easiest place to start if you've never run a
            blockchain node before.
          </p>
          <div className="flex flex-wrap gap-2">
            <a
              href="https://github.com/zcash/lightwalletd"
              target="_blank"
              rel="noreferrer"
              className="btn-outline"
            >
              Official lightwalletd repo ↗
            </a>
            <Link to="/run-node" className="btn-outline">Compare with full node</Link>
            <Link to="/register" className="btn-primary">After install → Register</Link>
          </div>
        </div>
        <ResourceVisual />
      </section>

      <section className="grid gap-4 md:grid-cols-2">
        <article className="card flex flex-col gap-3">
          <h2 className="text-lg font-semibold">What is lightwalletd?</h2>
          <p className="text-sm text-zcash-subtle">
            A gRPC server that translates the heavy state of a Zcash full node into
            compact blocks light wallets can sync quickly. It does not validate
            consensus itself — it relies on a backing <strong className="text-zcash-text">Zebra</strong> or
            <code className="mx-1 text-zcash-text">zcashd</code> for that. Light wallets
            connect to lightwalletd over gRPC and get only the data they need.
          </p>
        </article>

        <article className="card flex flex-col gap-3">
          <h2 className="text-lg font-semibold">Why operate one</h2>
          <ul className="flex flex-col gap-2 text-sm text-zcash-subtle">
            <li>
              <span className="text-zcash-text font-medium">Lower hardware bar.</span>{" "}
              Disk and RAM needs are a fraction of a full node (you still need one alongside it).
            </li>
            <li>
              <span className="text-zcash-text font-medium">More privacy infrastructure.</span>{" "}
              Light wallet users pick which lightwalletd to trust. More public instances = better.
            </li>
            <li>
              <span className="text-zcash-text font-medium">DePINZcash rewards.</span>{" "}
              Paid in $ZePIN every snapshot cycle, like a full node but at a lower tier.
            </li>
          </ul>
        </article>
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="text-lg font-semibold">Resource footprint</h2>
        <p className="text-sm text-zcash-subtle">
          Lightwalletd itself is small — the disk number below is its compact-block cache
          on top of a backing Zebra/zcashd.
        </p>
        <div className="grid gap-3 md:grid-cols-2">
          <FootprintCard
            kind="Lightwalletd"
            gigs={30}
            ram="1–2 GB"
            note="Compact-block cache. Stable size."
            highlight
          />
          <FootprintCard
            kind="Backing Zebra (required)"
            gigs={120}
            ram="4–8 GB"
            note="A lightwalletd node still needs a full node behind it."
          />
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="text-lg font-semibold">Prerequisites</h2>
        <div className="card text-sm text-zcash-subtle">
          <ol className="flex list-decimal flex-col gap-1 pl-5 marker:text-zcash-gold">
            <li>A running Zebra full node with JSON-RPC enabled (see <Link to="/run-node" className="text-zcash-gold hover:underline">/run-node</Link>).</li>
            <li>Go 1.21+ (for source builds) or Docker.</li>
            <li>~30 GB free disk for the lightwalletd cache, on top of Zebra's chain storage.</li>
          </ol>
        </div>
      </section>

      <section className="flex flex-col gap-4">
        <h2 className="text-lg font-semibold">Install lightwalletd</h2>
        <div className="flex flex-wrap gap-2">
          <Tab active={method === "source"} onClick={() => setMethod("source")} label="Build from source" />
          <Tab active={method === "docker"} onClick={() => setMethod("docker")} label="Docker" />
        </div>

        {method === "source" && (
          <Card>
            <p className="text-sm text-zcash-subtle">
              Lightwalletd is a small Go program — build straight from the official repo.
            </p>
            <Code lang="bash">{`# Install Go (macOS via Homebrew shown; Linux: apt/yum)
brew install go

# Clone + build
git clone https://github.com/zcash/lightwalletd.git
cd lightwalletd
make

# Install the binary
sudo install ./lightwalletd /usr/local/bin/
lightwalletd --help`}</Code>
          </Card>
        )}

        {method === "docker" && (
          <Card>
            <p className="text-sm text-zcash-subtle">
              Easiest if you already have Docker. Uses the official image, persists the
              cache + config in named volumes.
            </p>
            <Code lang="bash">{`docker run -d \\
  --name lightwalletd \\
  --restart unless-stopped \\
  -p 9067:9067 \\
  -v lwd-cache:/srv/lightwalletd/db \\
  -v lwd-conf:/srv/lightwalletd/conf \\
  electriccoinco/lightwalletd:latest \\
  --conf-file /srv/lightwalletd/conf/zcash.conf \\
  --data-dir /srv/lightwalletd/db \\
  --grpc-bind-addr 0.0.0.0:9067`}</Code>
            <p className="text-xs text-zcash-subtle">
              You'll still need a <code className="text-zcash-text">zcash.conf</code> in the
              <code className="text-zcash-text">lwd-conf</code> volume pointing at your
              Zebra/zcashd. See the next section.
            </p>
          </Card>
        )}
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="text-lg font-semibold">Configure + run</h2>
        <Card>
          <ol className="flex list-decimal flex-col gap-3 pl-5 text-sm text-zcash-subtle marker:text-zcash-gold">
            <li>
              <span className="text-zcash-text font-medium">Point it at your Zebra RPC.</span>{" "}
              Create a minimal <code className="text-zcash-text">zcash.conf</code>:
              <Code lang="ini">{`# ~/.depinzcash/zcash.conf
rpcuser=user
rpcpassword=pass
rpcbind=127.0.0.1
rpcport=8232`}</Code>
              Zebra ignores the username/password, but lightwalletd requires the fields exist.
            </li>
            <li>
              <span className="text-zcash-text font-medium">Pick a cache dir.</span>{" "}
              <Code lang="bash">{`mkdir -p ~/.depinzcash/lwd-cache`}</Code>
            </li>
            <li>
              <span className="text-zcash-text font-medium">Start it.</span>{" "}
              <Code lang="bash">{`lightwalletd \\
  --conf-file ~/.depinzcash/zcash.conf \\
  --data-dir ~/.depinzcash/lwd-cache \\
  --grpc-bind-addr 127.0.0.1:9067 \\
  --no-tls-very-insecure   # only for local-only testing`}</Code>
              The first ingestion takes 1–2 hours as it builds the compact-block cache from
              your Zebra. After that it stays in sync at a few seconds per block.
            </li>
            <li>
              <span className="text-zcash-text font-medium">Health check.</span>{" "}
              <Code lang="bash">{`# basic gRPC health probe (needs grpcurl)
brew install grpcurl
grpcurl -plaintext 127.0.0.1:9067 cash.z.wallet.sdk.rpc.CompactTxStreamer.GetLightdInfo`}</Code>
            </li>
          </ol>
        </Card>
      </section>

      <section className="grid gap-4 md:grid-cols-3">
        <NextStep
          step="1"
          title="Register your node"
          body='Connect Phantom or use the relay CLI. Pick "lightwalletd" as the kind.'
          to="/register"
          cta="Open register →"
        />
        <NextStep
          step="2"
          title="Build depinzcash-relay"
          body="Same relay binary as the full-node path. Signs and submits proofs to the server."
          href="https://github.com/ZcashDePIN/DePINZcash"
          cta="Repo ↗"
        />
        <NextStep
          step="3"
          title="Watch your points"
          body="Lightwalletd is a lower reward tier than a full node. Combine both for more $ZePIN."
          to="/dashboard"
          cta="Open dashboard →"
        />
      </section>
    </div>
  );
}

function Tab({ active, onClick, label }: { active: boolean; onClick: () => void; label: string }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`rounded-md border px-3 py-1.5 text-sm transition-colors ${
        active
          ? "border-zcash-gold bg-zcash-gold/10 text-zcash-gold"
          : "border-zcash-border bg-transparent text-zcash-subtle hover:text-zcash-text"
      }`}
    >
      {label}
    </button>
  );
}

function Card({ children }: { children: React.ReactNode }) {
  return <div className="card flex flex-col gap-3">{children}</div>;
}

function Code({ children, lang }: { children: string; lang?: string }) {
  return (
    <pre className="overflow-x-auto rounded-md border border-zcash-border bg-zcash-dark p-3 font-mono text-xs leading-5 text-zcash-text">
      {lang && <div className="mb-1 text-[10px] uppercase tracking-wider text-zcash-subtle">{lang}</div>}
      <code>{children}</code>
    </pre>
  );
}

function FootprintCard({
  kind,
  gigs,
  ram,
  note,
  highlight,
}: {
  kind: string;
  gigs: number;
  ram: string;
  note: string;
  highlight?: boolean;
}) {
  const REFERENCE_GB = 250;
  const pct = Math.min(100, Math.round((gigs / REFERENCE_GB) * 100));
  return (
    <div className={`card flex flex-col gap-3 ${highlight ? "border-zcash-gold/40" : ""}`}>
      <h3 className="text-sm font-semibold">{kind}</h3>
      <div className="flex flex-col gap-1">
        <div className="flex items-baseline justify-between">
          <span className="stat-label">Disk</span>
          <span className="text-2xl font-semibold text-zcash-text">
            {gigs} <span className="text-sm text-zcash-subtle">GB</span>
          </span>
        </div>
        <div className="h-2 w-full overflow-hidden rounded-full bg-zcash-dark">
          <div
            className={`h-full rounded-full transition-all ${highlight ? "bg-zcash-gold" : "bg-zcash-subtle/60"}`}
            style={{ width: `${pct}%` }}
          />
        </div>
        <div className="flex justify-between text-[10px] text-zcash-subtle">
          <span>0</span>
          <span>{REFERENCE_GB} GB</span>
        </div>
      </div>
      <div>
        <span className="stat-label">RAM</span>
        <div className="text-sm font-medium text-zcash-text">{ram}</div>
      </div>
      <p className="text-xs text-zcash-subtle">{note}</p>
    </div>
  );
}

function NextStep({
  step,
  title,
  body,
  to,
  href,
  cta,
}: {
  step: string;
  title: string;
  body: string;
  to?: string;
  href?: string;
  cta: string;
}) {
  return (
    <div className="card flex flex-col gap-2">
      <span className="text-xs text-zcash-gold">Step {step}</span>
      <h3 className="text-base font-semibold">{title}</h3>
      <p className="text-sm text-zcash-subtle">{body}</p>
      <div className="mt-auto pt-2">
        {to ? (
          <Link to={to} className="text-sm text-zcash-gold hover:underline">{cta}</Link>
        ) : (
          <a href={href} target="_blank" rel="noreferrer" className="text-sm text-zcash-gold hover:underline">
            {cta}
          </a>
        )}
      </div>
    </div>
  );
}

function ResourceVisual() {
  return (
    <div className="card flex h-full flex-col gap-3">
      <div className="rounded-md border border-zcash-border bg-zcash-dark p-4">
        <div className="grid grid-cols-2 gap-3 text-xs text-zcash-subtle">
          <Pill label="lightwalletd" sub="gRPC :9067" />
          <Pill label="Backing Zebra" sub="JSON-RPC :8232" />
          <Pill label="Compact block cache" sub="~30 GB" />
          <Pill label="Serves light wallets" sub="Ywallet, Nighthawk" />
        </div>
      </div>
      <div className="rounded-md border border-zcash-border bg-zcash-dark p-4 text-xs text-zcash-subtle">
        <div className="mb-2 text-zcash-text">Wallets sync from your lightwalletd</div>
        <div className="font-mono leading-5">
          <span className="text-zcash-gold">→</span> GetBlockRange (compact blocks)
          <br />
          <span className="text-zcash-gold">←</span> stream of CompactBlock { }
        </div>
      </div>
    </div>
  );
}

function Pill({ label, sub }: { label: string; sub: string }) {
  return (
    <div className="rounded-md border border-zcash-border bg-zcash-surface px-3 py-2">
      <div className="text-zcash-text">{label}</div>
      <div className="text-[10px] text-zcash-subtle">{sub}</div>
    </div>
  );
}
