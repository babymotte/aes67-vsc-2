import { createEffect, createSignal } from "solid-js";
import { subscribeLs } from "./worterbuch";
import axios from "axios";

const [appName, setAppName] = createSignal<string | null>(null);
const [selectedVsc, setSelectedVsc] = createSignal<string | null>(null);
const [vscs, setVscs] = createSignal<string[]>([]);

axios.get("/api/v1/backend/app-name").then((response) => {
  setAppName(response.data);
});

createEffect(() => {
  let name = appName();
  if (name) {
    subscribeLs(name, setVscs);
  }
});

createEffect(() => {
  let vscList = vscs();
  if (vscList.length > 0 && !selectedVsc()) {
    setSelectedVsc(vscList[0]);
  }
});

export { vscs, appName, selectedVsc };
