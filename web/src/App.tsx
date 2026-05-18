import { Routes, Route } from "react-router-dom";

import { AppHeader } from "./components/AppHeader";
import { AppFooter } from "./components/AppFooter";
import { Home } from "./pages/Home";
import { Register } from "./pages/Register";
import { Dashboard } from "./pages/Dashboard";
import { Leaderboard } from "./pages/Leaderboard";
import { NotFound } from "./pages/NotFound";

export function App() {
  return (
    <div className="flex min-h-full flex-col">
      <AppHeader />
      <main className="mx-auto w-full max-w-6xl flex-1 px-4 py-8">
        <Routes>
          <Route path="/" element={<Home />} />
          <Route path="/register" element={<Register />} />
          <Route path="/dashboard" element={<Dashboard />} />
          <Route path="/dashboard/:wallet" element={<Dashboard />} />
          <Route path="/leaderboard" element={<Leaderboard />} />
          <Route path="*" element={<NotFound />} />
        </Routes>
      </main>
      <AppFooter />
    </div>
  );
}
