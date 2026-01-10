import { Suspense, Switch, Match } from "solid-js";
import Receivers from "./components/Receivers/Receivers";
import Senders from "./components/Senders/Senders";
import Config from "./components/Config/Config";
import "./App.css";
import { appName, running } from "./vscState";
import { connected, get, locked, set } from "./worterbuch";
import Indicator from "./components/Indicator";
import { useNavigate } from "@solidjs/router";

function AddSenderButton() {
  return (
    <button
      onclick={async () => {
        const an = appName();
        console.log("Add sender ...");
        let id = await locked(`${appName()}/config/tx/next-id`, async () => {
          const id = (await get<number>(`${appName()}/config/tx/next-id`)) || 1;
          set(`${an}/config/tx/next-id`, id + 1);
          return id;
        });
        if (id != null) {
          set(`${an}/config/tx/senders/${id}/name`, null);
          set(`${an}/config/tx/senders/${id}/running`, false);
        }
      }}
    >
      +
    </button>
  );
}

function AddReceiverButton() {
  return (
    <button
      onclick={async () => {
        const an = appName();
        console.log("Add receiver ...");
        let id = await locked(`${appName()}/config/rx/next-id`, async () => {
          const id = (await get<number>(`${appName()}/config/rx/next-id`)) || 1;
          set(`${an}/config/rx/next-id`, id + 1);
          return id;
        });
        if (id != null) {
          set(`${an}/config/rx/receivers/${id}/name`, null);
          set(`${an}/config/rx/receivers/${id}/running`, false);
        }
      }}
    >
      +
    </button>
  );
}

export default function App(props: { tab?: number }) {
  const navigate = useNavigate();

  return (
    <>
      <div class="header">
        <ul class="main-menu">
          <li
            classList={{ selected: props.tab == null || props.tab === 0 }}
            onClick={() => navigate("/tx")}
          >
            Senders
          </li>
          <li
            classList={{ selected: props.tab === 1 }}
            onClick={() => navigate("/rx")}
          >
            Receivers
          </li>
          <li
            classList={{ selected: props.tab === 2 }}
            onClick={() => navigate("/config")}
          >
            Config
          </li>
        </ul>
        <Switch>
          <Match when={props.tab === 0}>
            <AddSenderButton />
          </Match>
          <Match when={props.tab === 1}>
            <AddReceiverButton />
          </Match>
        </Switch>
        <div class="spacer"></div>
        <div>
          <Indicator onLabel="Backend " offLabel="Backend " on={connected} />
          <Indicator onLabel="VSC " offLabel="VSC " on={running} />
        </div>
      </div>
      <div class="tab">
        <Suspense fallback={<div class="loader">Loading...</div>}>
          <Switch>
            <Match when={props.tab === 0}>
              <Senders />
            </Match>
            <Match when={props.tab === 1}>
              <Receivers />
            </Match>
            <Match when={props.tab === 2}>
              <Config />
            </Match>
          </Switch>
        </Suspense>
      </div>
    </>
  );
}
