import { createEffect, createSignal } from "solid-js";
import axios from "axios";
import { subscribe } from "./worterbuch";

const [appName, setAppName] = createSignal<string | null>(null);
const [running, setRunning] = createSignal<boolean>(false);

export function VscState() {
  axios.get("/api/v1/backend/app-name").then((response) => {
    setAppName(response.data);
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

  return null;
}

export { appName, running };
