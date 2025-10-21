import {
  createEffect,
  createSignal,
  For,
  Match,
  Suspense,
  Switch,
  useTransition,
} from "solid-js";
import { pSubscribe } from "../worterbuch";
import { selectedVsc, appName } from "../vscState";
import { sortTransceivers, transceiverID } from "../utils";

export default function Receivers() {
  const [tab, setTab] = createSignal(0);
  const [pending, start] = useTransition();
  const [receivers, setReceivers] = createSignal<Map<string, string>>(
    new Map()
  );
  const updateTab = (index: number) => () => start(() => setTab(index));

  createEffect(() => {
    const an = appName();
    const sv = selectedVsc();
    pSubscribe(`${an}/${sv}/rx/?/label`, setReceivers);
  });

  return (
    <div class="tab-content">
      <div class="sub-menu">
        <ul>
          <For each={sortTransceivers(Array.from(receivers().entries()))}>
            {(receiver, index) => (
              <li
                classList={{ selected: tab() === index() }}
                onClick={updateTab(index())}
              >
                {transceiverID(receiver[0])} - {receiver[1]}
              </li>
            )}
          </For>
        </ul>
      </div>
      <div class="main-view" classList={{ pending: pending() }}>
        <Suspense fallback={<div class="loader">Loading...</div>}>
          <Switch>
            <For each={sortTransceivers(Array.from(receivers().entries()))}>
              {(receiver, index) => (
                <Match when={tab() === index()}>
                  <h3>{receiver[1]}</h3>
                </Match>
              )}
            </For>
          </Switch>
        </Suspense>
      </div>
    </div>
  );
}
