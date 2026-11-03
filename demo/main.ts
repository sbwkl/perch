import { mount } from "svelte";
import App from "../src/App.svelte";
import "../src/app.css";

const app = mount(App, {
  target: document.getElementById("app")!,
});

function waitFor<T extends Element>(
  finder: () => T | null,
  timeout = 3_000,
): Promise<T | null> {
  return new Promise((resolve) => {
    const started = Date.now();
    const timer = setInterval(() => {
      const found = finder();
      if (found || Date.now() - started > timeout) {
        clearInterval(timer);
        resolve(found);
      }
    }, 30);
  });
}

async function drive() {
  const params = new URLSearchParams(location.search);
  const view = params.get("view");
  const select = params.get("select");

  if (view) {
    const button = await waitFor(() =>
      [...document.querySelectorAll<HTMLButtonElement>(".nav-item")].find(
        (item) => item.querySelector(".nav-label")?.textContent?.trim() === view,
      ) ?? null,
    );
    button?.click();
    await new Promise((resolve) => setTimeout(resolve, 120));
  }

  if (select) {
    const button = await waitFor(() =>
      [...document.querySelectorAll<HTMLButtonElement>(".task")].find((item) =>
        item.querySelector(".task-title")?.textContent?.includes(select),
      ) ?? null,
    );
    button?.click();
  }
}

void drive();

export default app;
