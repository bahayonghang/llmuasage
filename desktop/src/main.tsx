import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Shell } from "./app/shell";
import "./styles/tokens.css";
import "./styles/layout.css";

document.documentElement.setAttribute("data-theme", "dark");

const root = document.getElementById("root");
if (!root) {
  throw new Error("missing #root");
}

createRoot(root).render(
  <StrictMode>
    <Shell />
  </StrictMode>,
);
