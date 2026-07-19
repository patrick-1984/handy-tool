import React from "react";
import ReactDOM from "react-dom/client";
import FloatingTranscription from "./FloatingTranscription";
import "@/i18n";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <FloatingTranscription />
  </React.StrictMode>,
);
