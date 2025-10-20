import axios from "axios";
import { createEffect, createSignal, onCleanup } from "solid-js";
import {
  connect,
  type LsCallback,
  type StateCallback,
  type TransactionID,
  type Value,
  type Worterbuch,
} from "worterbuch-js";

const [wbServers, setWbServers] = createSignal<string[]>([]);
const [wbClient, setWbClient] = createSignal<Worterbuch | null>(null);

axios.get("/api/v1/backend/wb-servers").then((response) => {
  setWbServers(response.data.split(",").map((s: string) => s.trim()));
});

createEffect(() => {
  const servers = wbServers();
  console.log("WB Servers:", servers);
  connect(wbServers().map((s: string) => `ws://${s}/ws`))
    .then((wb) => {
      setWbClient(wb);
    })
    .catch((err) => {
      console.error("Failed to connect to Worterbuch:", err.message);
      setWbClient(null);
    });
});

createEffect(() => {
  const wb = wbClient();
  if (wb) {
    wb.onclose = () => {
      // TODO reconnect logic
      console.warn("Worterbuch connection closed");
      setWbClient(null);
    };
  }
});

export function subscribe<T extends Value>(key: string, cb: StateCallback<T>) {
  const [tid, setTid] = createSignal<TransactionID | null>(null);
  createEffect(() => {
    const wb = wbClient();
    if (wb) {
      setTid(wb.subscribe(key, cb));
    }
  });
  onCleanup(() => {
    const wb = wbClient();
    const id = tid();
    if (wb && id) {
      wb.unsubscribe(id);
    }
  });
}

export function subscribeLs(parent: string, cb: LsCallback) {
  const [tid, setTid] = createSignal<TransactionID | null>(null);
  createEffect(() => {
    const wb = wbClient();
    if (wb) {
      setTid(wb.subscribeLs(parent, cb));
    }
  });
  onCleanup(() => {
    const wb = wbClient();
    const id = tid();
    if (wb && id) {
      wb.unsubscribeLs(id);
    }
  });
}
