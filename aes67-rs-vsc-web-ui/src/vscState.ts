import { createEffect, createSignal } from "solid-js";
import { connected, subscribe } from "./worterbuch";
import { fetchAppName } from "./api";

const [appName, setAppName] = createSignal<string | null>(null);
const [running, setRunning] = createSignal<boolean>(false);

createEffect(() => {
  if (connected()) {
    fetchAppName()
      .then((name) => {
        setAppName(name);
      })
      .catch((error) => {
        console.error("Error fetching app name:", error);
      });
  } else {
    setAppName(null);
  }
});

createEffect(() => {
  const an = appName();
  if (an) {
    console.log("App name changed:", an);
    subscribe<boolean>(`${an}/running`, (val) => {
      console.log("VSC running:", val.value);
      setRunning(val.value || false);
    });
  }
});

export { appName, running };
