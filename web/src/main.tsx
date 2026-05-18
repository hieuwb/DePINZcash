import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";

import { WalletProvider } from "./lib/wallet";
import { App } from "./App";
import "@solana/wallet-adapter-react-ui/styles.css";
import "./styles.css";

const root = document.getElementById("root");
if (!root) {
  throw new Error("missing #root element");
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <BrowserRouter>
      <WalletProvider>
        <App />
      </WalletProvider>
    </BrowserRouter>
  </React.StrictMode>,
);
