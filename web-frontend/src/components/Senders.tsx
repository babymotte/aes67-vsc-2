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

export default function Senders() {
  const [tab, setTab] = createSignal(0);
  const [pending, start] = useTransition();
  const [senders, setSenders] = createSignal<Map<string, string>>(new Map());
  const updateTab = (index: number) => () => start(() => setTab(index));

  createEffect(() => {
    const an = appName();
    const sv = selectedVsc();
    pSubscribe(`${an}/${sv}/tx/?/label`, setSenders);
  });

  return (
    <div class="tab-content">
      <div class="sub-menu">
        <ul>
          <For each={sortTransceivers(Array.from(senders().entries()))}>
            {(sender, index) => (
              <li
                classList={{ selected: tab() === index() }}
                onClick={updateTab(index())}
              >
                {transceiverID(sender[0])} - {sender[1]}
              </li>
            )}
          </For>
        </ul>
      </div>
      <div class="main-view" classList={{ pending: pending() }}>
        <Suspense fallback={<div class="loader">Loading...</div>}>
          <Switch>
            <For each={sortTransceivers(Array.from(senders().entries()))}>
              {(sender, index) => (
                <Match when={tab() === index()}>
                  <h3>{sender[1]}</h3>
                </Match>
              )}
            </For>
          </Switch>
        </Suspense>
      </div>
    </div>
  );
}
