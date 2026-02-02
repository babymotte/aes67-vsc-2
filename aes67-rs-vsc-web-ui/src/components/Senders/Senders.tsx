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
import {
  createWbSignal,
  sortSenders,
  transceiverID,
  transceiverLabel,
} from "../../utils";
import Editor from "./Editor";
import Indicator from "../Indicator";

export default function Senders(props: {
  tabSignal: [Accessor<number>, Setter<number>];
}) {
  const [tab, setTab] = props.tabSignal;
  const [pending, start] = useTransition();
  const [senders, setSenders] = createSignal<Map<string, string>>(new Map());
  const [sortedSenders, setSortedSenders] = createSignal<[string, string][]>(
    [],
  );
  const updateTab = (index: number) => () => start(() => setTab(index));

  createEffect(() => {
    pSubscribe(`${appName()}/config/tx/?/name`, setSenders);
  });

  createEffect(() => {
    setSortedSenders(sortSenders(Array.from(senders().entries())));
  });

  createEffect(() => {
    if (tab() >= sortedSenders().length) {
      setTimeout(() => setTab(sortedSenders().length - 1), 100);
    }
  });

  function SenderTab(props: {
    sender: [string, string];
    index: Accessor<number>;
  }) {
    const [running] = createWbSignal(
      `/tx/${transceiverID(props.sender)}/running`,
      false,
    );
    return (
      <li
        classList={{ selected: tab() === props.index() }}
        onClick={updateTab(props.index())}
      >
        <Indicator
          onLabel={transceiverLabel(props.sender)}
          offLabel={transceiverLabel(props.sender)}
          on={running}
        />
      </li>
    );
  }

  return (
    <div class="tab-content">
      <div class="sub-menu">
        <ul>
          <For each={sortedSenders()}>
            {(sender, index) => <SenderTab sender={sender} index={index} />}
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
