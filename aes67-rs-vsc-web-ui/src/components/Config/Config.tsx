import { createSignal, Match, Suspense, Switch, useTransition } from "solid-js";
import VSC from "./VSC";
import { running } from "../../vscState";
import JACK from "./JACK";

export default function Config() {
  const [tab, setTab] = createSignal(0);
  const [pending, start] = useTransition();
  const updateTab = (index: number, disableWhenRunning: boolean) => () => {
    if (disableWhenRunning && running()) {
      return;
    }
    start(() => setTab(index));
  };

  return (
    <div class="tab-content">
      <div class="sub-menu">
        <ul>
          <li
            classList={{ selected: tab() === 0 }}
            onClick={updateTab(0, false)}
          >
            VSC
          </li>
          <li
            classList={{ selected: tab() === 1 }}
            onClick={updateTab(1, false)}
          >
            JACK
          </li>
        </ul>
      </div>
      <div class="main-view" classList={{ pending: pending() }}>
        <Suspense fallback={<div class="loader">Loading...</div>}>
          <Switch>
            <Match when={tab() === 0}>
              <VSC />
            </Match>
            <Match when={tab() === 1}>
              <JACK />
            </Match>
          </Switch>
        </Suspense>
      </div>
    </div>
  );
}
