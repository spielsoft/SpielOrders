import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";

function App() {
  return (
    <main>
      <h1>SpielOrders</h1>
      <p>The desktop shell is ready. Package behavior begins in a later milestone.</p>
    </main>
  );
}

const root = document.getElementById("root");
if (!root) throw new Error("missing React root");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>
);

