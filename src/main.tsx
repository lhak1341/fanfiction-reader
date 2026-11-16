import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { KitRoot, type ModeChoice } from "@lhak/style/react";
// The token contract first: app.css builds on it.
import "@lhak/style/tokens.css";
// The kit's bundled fonts: `font-src 'self'` allows no web font service.
import "@lhak/style/fonts.css";
// Rules Base UI would inject as <style>, which `style-src 'self'` blocks; KitRoot stops the injection.
import "@lhak/style/kit.css";
import "./app.css";
import { getTheme } from "$lib/theme";
import { App } from "./app/App";

// The Svelte app saved light/dark/system under "ffn-reader-theme". The kit
// remembers the user's next pick under its own key; until then this keeps the
// mode they had.
const mode: ModeChoice = getTheme();

createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <KitRoot defaultTheme={{ flavor: "capri", mode }}>
      <App />
    </KitRoot>
  </StrictMode>
);
