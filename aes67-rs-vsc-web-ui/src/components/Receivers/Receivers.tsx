import {
  createEffect,
  createSignal,
  For,
  Match,
  Suspense,
  Switch,
  useTransition,
  type Accessor,
  type Setter,
} from "solid-js";
import { pSubscribe } from "../../worterbuch";
import { appName } from "../../vscState";
import { sortReceivers, transceiverLabel } from "../../utils";
import Editor from "./Editor";

export default function Receivers(props: {
  tabSignal: [Accessor<number>, Setter<number>];
}) {
  const [tab, setTab] = props.tabSignal;
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

  createEffect(() => {
    if (tab() >= sortedReceivers().length) {
      setTimeout(() => setTab(sortedReceivers().length - 1), 100);
    }
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
                  <Editor receiver={receiver} />
                </Match>
              )}
            </For>
          </Switch>
        </Suspense>
      </div>
    </div>
  );
}
