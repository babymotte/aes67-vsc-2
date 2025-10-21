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

export default function Receivers() {
  const [tab, setTab] = createSignal(0);
  const [pending, start] = useTransition();
  const [receivers, setReceivers] = createSignal<string[]>([]);
  const updateTab = (index: number) => () => start(() => setTab(index));

  createEffect(() => {
    const an = appName();
    const sv = selectedVsc();
    subscribeLs(`${an}/${sv}/rx`, setReceivers);
  });

  return (
    <div class="tab-content">
      <div class="sub-menu">
        <ul>
          <For each={receivers().sort()}>
            {(receiver, index) => (
              <li
                classList={{ selected: tab() === index() }}
                onClick={updateTab(index())}
              >
                {receiver}
              </li>
            )}
          </For>
        </ul>
      </div>
      <div class="main-view" classList={{ pending: pending() }}>
        <Suspense fallback={<div class="loader">Loading...</div>}>
          <Switch>
            <For each={receivers().sort()}>
              {(receiver, index) => (
                <Match when={tab() === index()}>
                  <h3>{receiver}</h3>
                </Match>
              )}
            </For>
          </Switch>
        </Suspense>
      </div>
    </div>
  );
}
