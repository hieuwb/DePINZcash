import { Link } from "react-router-dom";

export function NotFound() {
  return (
    <div className="card flex flex-col items-start gap-3">
      <h1 className="text-xl font-semibold">Not found</h1>
      <p className="text-sm text-zcash-subtle">That route doesn't exist.</p>
      <Link to="/" className="btn-outline">Back home</Link>
    </div>
  );
}
