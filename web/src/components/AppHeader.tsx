import { NavLink } from "react-router-dom";
import { WalletMultiButton } from "@solana/wallet-adapter-react-ui";

const links = [
  { to: "/", label: "Home", end: true },
  { to: "/run-node", label: "Run a node" },
  { to: "/run-lightwalletd", label: "Run lightwalletd" },
  { to: "/leaderboard", label: "Leaderboard" },
  { to: "/explorer", label: "Explorer" },
  { to: "/dashboard", label: "Dashboard" },
  { to: "/register", label: "Register node" },
];

export function AppHeader() {
  return (
    <header className="border-b border-zcash-border bg-zcash-dark/95 backdrop-blur">
      <div className="mx-auto flex max-w-6xl items-center justify-between px-4 py-3">
        <NavLink to="/" className="flex items-center gap-2">
          <img
            src="/main.svg"
            alt="DePINZcash logo"
            className="h-7 w-7 rounded-md object-contain bg-transparent"
          />
          <span className="text-lg font-semibold tracking-tight">DePINZcash</span>
          <span className="hidden text-xs text-zcash-subtle sm:inline">/ $ZePIN on Solana</span>
        </NavLink>
        <nav className="hidden gap-1 md:flex">
          {links.map((l) => (
            <NavLink
              key={l.to}
              to={l.to}
              end={l.end}
              className={({ isActive }) =>
                `rounded-md px-3 py-1.5 text-sm transition-colors ${
                  isActive
                    ? "bg-zcash-surface text-zcash-text"
                    : "text-zcash-subtle hover:text-zcash-text"
                }`
              }
            >
              {l.label}
            </NavLink>
          ))}
        </nav>
        <WalletMultiButton />
      </div>
      <div className="md:hidden">
        <div className="mx-auto flex max-w-6xl gap-2 overflow-x-auto px-4 pb-3">
          {links.map((l) => (
            <NavLink
              key={l.to}
              to={l.to}
              end={l.end}
              className={({ isActive }) =>
                `shrink-0 rounded-md px-3 py-1 text-sm ${
                  isActive
                    ? "bg-zcash-surface text-zcash-text"
                    : "text-zcash-subtle hover:text-zcash-text"
                }`
              }
            >
              {l.label}
            </NavLink>
          ))}
        </div>
      </div>
    </header>
  );
}

