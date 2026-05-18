export function AppFooter() {
  return (
    <footer className="border-t border-zcash-border bg-zcash-dark/95">
      <div className="mx-auto flex max-w-6xl flex-col gap-2 px-4 py-6 text-xs text-zcash-subtle md:flex-row md:items-center md:justify-between">
        <span>
          DePINZcash · incentive layer for Zebra full nodes. Rewards paid in an SPL token
          on Solana until NU7 / ZIP-227 land custom assets on Zcash.
        </span>
        <span>
          MIT · <a className="hover:text-zcash-text" href="https://zips.z.cash/zip-0227" target="_blank" rel="noreferrer">ZIP-227</a> ·{" "}
          <a className="hover:text-zcash-text" href="https://github.com/ZcashFoundation/zebra" target="_blank" rel="noreferrer">Zebra</a>
        </span>
      </div>
    </footer>
  );
}
