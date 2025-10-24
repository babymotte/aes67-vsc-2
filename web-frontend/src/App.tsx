import { createSignal, Suspense, Switch, Match, useTransition } from "solid-js";
import Receivers from "./components/Receivers";
import Senders from "./components/Senders";
import Config from "./components/Config";
import "./App.css";
import { VscState } from "./vscState";
import { Worterbuch } from "./worterbuch";

function AddSenderButton() {
  return <button onclick={() => console.log("Add sender ...")}>+</button>;
}

function AddReceiverButton() {
  return <button onclick={() => console.log("Add receiver ...")}>+</button>;
}

export default function App() {
  const [tab, setTab] = createSignal(0);
  const [pending, start] = useTransition();
  const updateTab = (index: number) => () => start(() => setTab(index));

  return (
    <>
      <Worterbuch />
      <VscState />
      <div class="header">
        <ul class="main-menu">
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
        <Switch>
          <Match when={tab() === 0}>
            <AddSenderButton />
          </Match>
          <Match when={tab() === 1}>
            <AddReceiverButton />
          </Match>
        </Switch>
      </div>
      <div class="tab" classList={{ pending: pending() }}>
        <Suspense fallback={<div class="loader">Loading...</div>}>
          <Switch>
            <Match when={tab() === 0}>
              <Senders />
            </Match>
            <Match when={tab() === 1}>
              <Receivers />
            </Match>
            <Match when={tab() === 2}>
              <Config />
            </Match>
          </Switch>
        </Suspense>
      </div>
    </>
  );
}
