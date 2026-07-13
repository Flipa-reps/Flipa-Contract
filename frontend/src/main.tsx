import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import "../tokens/flipa.tokens.css";
import "../typography/fonts.css";
import "../typography/typography.css";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
