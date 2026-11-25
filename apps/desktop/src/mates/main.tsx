// The Evo Mates window's page (mates.html): the second front end of the one
// app (docs/features/evo-mates/spec.md, FR-1, FR-3; ADR 0028).

import * as React from "react";
import ReactDOM from "react-dom/client";
import { Providers } from "@/app/providers";
import "../index.css";
// The accent, background and fonts the main window uses.
import "../stores/appearance-store";
import { MatesApp } from "./mates-app";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Providers>
      <MatesApp />
    </Providers>
  </React.StrictMode>,
);
