import { createSignal, Match, Suspense, Switch, useTransition } from "solid-js";
import Network from "./Network";

export default function Config() {
  const [tab, setTab] = createSignal(0);
  const [pending, start] = useTransition();
  const updateTab = (index: number) => () => start(() => setTab(index));

  return (
    <div class="tab-content">
      <div class="sub-menu">
        <ul>
          <li classList={{ selected: tab() === 0 }} onClick={updateTab(0)}>
            Network
          </li>
          <li classList={{ selected: tab() === 1 }} onClick={updateTab(1)}>
            UI
          </li>
          <li classList={{ selected: tab() === 2 }} onClick={updateTab(2)}>
            Backend
          </li>
        </ul>
      </div>
      <div class="main-view" classList={{ pending: pending() }}>
        <Suspense fallback={<div class="loader">Loading...</div>}>
          <Switch>
            <Match when={tab() === 0}>
              <Network />
            </Match>
            <Match when={tab() === 1}>
              <h3>World</h3>
            </Match>
            <Match when={tab() === 2}>
              <h3>There</h3>
            </Match>
          </Switch>
        </Suspense>
      </div>
    </div>
  );
}
