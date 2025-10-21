import {
  createEffect,
  createSignal,
  For,
  Match,
  Suspense,
  Switch,
  useTransition,
} from "solid-js";
import { subscribeLs } from "../worterbuch";
import { selectedVsc, appName } from "../vscState";

export default function Senders() {
  const [tab, setTab] = createSignal(0);
  const [pending, start] = useTransition();
  const [senders, setSenders] = createSignal<string[]>([]);
  const updateTab = (index: number) => () => start(() => setTab(index));

  createEffect(() => {
    const an = appName();
    const sv = selectedVsc();
    subscribeLs(`${an}/${sv}/tx`, setSenders);
  });

  return (
    <div class="tab-content">
      <div class="sub-menu">
        <ul>
          <For each={senders().sort()}>
            {(sender, index) => (
              <li
                classList={{ selected: tab() === index() }}
                onClick={updateTab(index())}
              >
                {sender}
              </li>
            )}
          </For>
        </ul>
      </div>
      <div class="main-view" classList={{ pending: pending() }}>
        <Suspense fallback={<div class="loader">Loading...</div>}>
          <Switch>
            <For each={senders().sort()}>
              {(sender, index) => (
                <Match when={tab() === index()}>
                  <h3>{sender}</h3>
                </Match>
              )}
            </For>
          </Switch>
        </Suspense>
      </div>
    </div>
  );
}
