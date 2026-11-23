// The quick entry box's page (quick-entry.html): kept small, it loads none of
// the main app's stores or engine code (docs/features/quick-entry, NFR-1).

import * as React from "react";
import ReactDOM from "react-dom/client";
import "../index.css";
// The accent, background and fonts the main window uses.
import "../stores/appearance-store";
import { QuickEntry } from "./quick-entry";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QuickEntry />
  </React.StrictMode>,
);
