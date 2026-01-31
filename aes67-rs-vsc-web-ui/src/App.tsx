import {
  Suspense,
  Switch,
  Match,
  createSignal,
  type Accessor,
  type Setter,
  For,
  createEffect,
} from "solid-js";
import Receivers from "./components/Receivers/Receivers";
import Senders from "./components/Senders/Senders";
import Config from "./components/Config/Config";
import "./App.css";
import { appName, running } from "./vscState";
import { connected, pSubscribe } from "./worterbuch";
import Indicator from "./components/Indicator";
import { useNavigate } from "@solidjs/router";
import {
  createReceiverConfig,
  createReceiverConfigForSession,
  createSenderConfig,
} from "./api";

function AddSenderButton(props: {
  tabSignal: [Accessor<number>, Setter<number>];
}) {
  return (
    <button
      onclick={() => {
        addSender(props.tabSignal[1]);
      }}
    >
      +
    </button>
  );
}

const addSender = async (setTab: Setter<number>) => {
  createSenderConfig().catch((error) => {
    console.error("Error creating sender config:", error);
    // TODO: show error to user
  });
  setTab(Number.MAX_SAFE_INTEGER);
};

const addReceiverFromSdp = async (setTab: Setter<number>) => {
  setCreateRcvSubmenuOpen(false);
  setSenderListOpen(false);
  // TODO: open dialog to get SDP content or URL
  console.log("Adding receiver from SDP ...");
  setTab(Number.MAX_SAFE_INTEGER);
};

const addReceiver = async (setTab: Setter<number>, sessionId?: string) => {
  console.log("sessionId", sessionId);

  setCreateRcvSubmenuOpen(false);
  setSenderListOpen(false);

  (sessionId
    ? createReceiverConfigForSession(sessionId)
    : createReceiverConfig()
  ).catch((error) => {
    console.error("Error creating receiver config:", error);
    // TODO: show error to user
  });
  setTab(Number.MAX_SAFE_INTEGER);
};

const [createRcvSubmenuOpen, setCreateRcvSubmenuOpen] =
  createSignal<boolean>(false);
const [senderListOpen, setSenderListOpen] = createSignal<boolean>(false);

function SessionList(props: { tabSignal: [Accessor<number>, Setter<number>] }) {
  const [sessions, setSessions] = createSignal<[string, string][]>([]);

  createEffect(() => {
    const an = appName();
    pSubscribe<string>(`${an}/discovery/sessions/?/name`, (sessionNames) => {
      const sessions = Array.from(sessionNames.entries());
      setSessions(sessions);
    });
  });

  return (
    <div class="dropdown-menu" classList={{ open: senderListOpen() }}>
      <For each={sessions()}>
        {(session) => (
          <div
            class="menuitem"
            onclick={() => {
              addReceiver(props.tabSignal[1], session[0].split("/")[3]);
            }}
          >
            {session[1]}
          </div>
        )}
      </For>
    </div>
  );
}

function AddReceiverButton(props: {
  tabSignal: [Accessor<number>, Setter<number>];
}) {
  return (
    <div>
      <button
        on:click={() => {
          setCreateRcvSubmenuOpen(!createRcvSubmenuOpen());
          setSenderListOpen(false);
        }}
      >
        {createRcvSubmenuOpen() ? "-" : "+"}
      </button>
      <div
        id="addRecvDropdown"
        class="dropdown-menu"
        classList={{ open: createRcvSubmenuOpen() }}
      >
        <div
          class="menuitem submenu"
          on:mouseenter={() => setSenderListOpen(true)}
          on:mouseleave={() => setSenderListOpen(false)}
        >
          <span>From Sender</span>
          <span>🢒</span>
          <SessionList tabSignal={props.tabSignal} />
        </div>
        <div
          class="menuitem"
          onclick={() => addReceiverFromSdp(props.tabSignal[1])}
        >
          From SDP
        </div>
        <div class="menuitem" onclick={() => addReceiver(props.tabSignal[1])}>
          Custom
        </div>
      </div>
    </div>
  );
}

export default function App(props: { tab?: number }) {
  const navigate = useNavigate();

  const senderTab = createSignal<number>(0);
  const receiverTab = createSignal<number>(0);

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
            <AddSenderButton tabSignal={senderTab} />
          </Match>
          <Match when={props.tab === 1}>
            <AddReceiverButton tabSignal={receiverTab} />
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
              <Senders tabSignal={senderTab} />
            </Match>
            <Match when={props.tab === 1}>
              <Receivers tabSignal={receiverTab} />
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
