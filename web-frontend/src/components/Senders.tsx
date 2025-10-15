import { createSignal, Match, Suspense, Switch, useTransition } from "solid-js";

export default function Senders() {
  const [tab, setTab] = createSignal(0);
  const [pending, start] = useTransition();
  const updateTab = (index: number) => () => start(() => setTab(index));

  return (
    <>
      <ul class="sub-menu">
        <li classList={{ selected: tab() === 0 }} onClick={updateTab(0)}>
          Senders
        </li>
        <li classList={{ selected: tab() === 1 }} onClick={updateTab(1)}>
          Receivers
        </li>
        <li classList={{ selected: tab() === 2 }} onClick={updateTab(2)}>
          Config
        </li>
      </ul>
      <div class="tab" classList={{ pending: pending() }}>
        <Suspense fallback={<div class="loader">Loading...</div>}>
          <Switch>
            <Match when={tab() === 0}>Hello</Match>
            <Match when={tab() === 1}>World</Match>
            <Match when={tab() === 2}>There</Match>
          </Switch>
        </Suspense>
      </div>
    </>
  );
}
