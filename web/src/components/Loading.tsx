export function Loading({ label }: { label?: string }) {
  return (
    <div className="flex items-center gap-2 text-sm text-zcash-subtle">
      <span className="inline-block h-3 w-3 animate-spin rounded-full border-2 border-zcash-gold border-t-transparent" />
      <span>{label ?? "loading…"}</span>
    </div>
  );
}

export function ErrorBanner({ message }: { message: string }) {
  return (
    <div className="rounded-md border border-zcash-danger/40 bg-zcash-danger/10 px-3 py-2 text-sm text-red-200">
      {message}
    </div>
  );
}
