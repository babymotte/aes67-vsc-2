import { createEffect, createSignal } from "solid-js";
import { subscribeLs } from "./worterbuch";
import axios from "axios";

const [appName, setAppName] = createSignal<string | null>(null);

axios.get("/api/v1/backend/app-name").then((response) => {
  setAppName(response.data);
});

createEffect(() => {
  let name = appName();
  if (name) {
    subscribeLs(name, setVscs);
  }
});

const [vscs, setVscs] = createSignal<string[]>([]);
subscribeLs("aes67-vsc", setVscs);

export { vscs };
