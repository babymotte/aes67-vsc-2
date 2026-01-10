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
import { sortSenders, transceiverLabel } from "../../utils";
import Editor from "./Editor";

export default function Senders() {
  const [tab, setTab] = createSignal(0);
  const [pending, start] = useTransition();
  const [senders, setSenders] = createSignal<Map<string, string>>(new Map());
  const [sortedSenders, setSortedSenders] = createSignal<[string, string][]>(
    []
  );
  const updateTab = (index: number) => () => start(() => setTab(index));

  createEffect(() => {
    pSubscribe(`${appName()}/config/tx/senders/?/name`, setSenders);
  });

  createEffect(() => {
    setSortedSenders(sortSenders(Array.from(senders().entries())));
  });

  return (
    <div class="tab-content">
      <div class="sub-menu">
        <ul>
          <For each={sortedSenders()}>
            {(sender, index) => (
              <li
                classList={{ selected: tab() === index() }}
                onClick={updateTab(index())}
              >
                {transceiverLabel(sender)}
              </li>
            )}
          </For>
        </ul>
      </div>
      <div class="main-view" classList={{ pending: pending() }}>
        <Suspense fallback={<div class="loader">Loading...</div>}>
          <Switch>
            <For each={sortedSenders()}>
              {(sender, index) => (
                <Match when={tab() === index()}>
                  <Editor sender={sender} />
                </Match>
              )}
            </For>
          </Switch>
        </Suspense>
      </div>
    </div>
  );
}
