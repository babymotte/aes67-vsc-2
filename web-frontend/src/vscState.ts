import { createSignal } from "solid-js";
import axios from "axios";

const [appName, setAppName] = createSignal<string | null>(null);

export function VscState() {
  axios.get("/api/v1/backend/app-name").then((response) => {
    setAppName(response.data);
  });

  return null;
}

export { appName };
