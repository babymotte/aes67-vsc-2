import {
  createEffect,
  createSignal,
  For,
  Match,
  Suspense,
  Switch,
  useTransition,
} from "solid-js";
import { pSubscribe } from "../../worterbuch";
import { appName } from "../../vscState";
import { sortReceivers, transceiverLabel } from "../../utils";

export default function Receivers() {
  const [tab, setTab] = createSignal(0);
  const [pending, start] = useTransition();
  const [receivers, setReceivers] = createSignal<Map<string, string>>(
    new Map()
  );
  const [sortedReceivers, setSortedReceivers] = createSignal<
    [string, string][]
  >([]);
  const updateTab = (index: number) => () => start(() => setTab(index));

  createEffect(() => {
    pSubscribe(`${appName()}/config/rx/receivers/?/name`, setReceivers);
  });

  createEffect(() => {
    setSortedReceivers(sortReceivers(Array.from(receivers().entries())));
  });

  return (
    <div class="tab-content">
      <div class="sub-menu">
        <ul>
          <For each={sortedReceivers()}>
            {(receiver, index) => (
              <li
                classList={{ selected: tab() === index() }}
                onClick={updateTab(index())}
              >
                {transceiverLabel(receiver)}
              </li>
            )}
          </For>
        </ul>
      </div>
      <div class="main-view" classList={{ pending: pending() }}>
        <Suspense fallback={<div class="loader">Loading...</div>}>
          <Switch>
            <For each={sortedReceivers()}>
              {(receiver, index) => (
                <Match when={tab() === index()}>
                  <h3>{transceiverLabel(receiver)}</h3>
                </Match>
              )}
            </For>
          </Switch>
        </Suspense>
      </div>
    </div>
  );
}
