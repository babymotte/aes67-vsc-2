import {
  Suspense,
  Switch,
  Match,
  createSignal,
  type Accessor,
  type Setter,
  For,
} from "solid-js";
import Receivers from "./components/Receivers/Receivers";
import Senders from "./components/Senders/Senders";
import Config from "./components/Config/Config";
import "./App.css";
import { appName, running } from "./vscState";
import { connected, get, locked, pSubscribe, set } from "./worterbuch";
import Indicator from "./components/Indicator";
import { useNavigate } from "@solidjs/router";

function AddSenderButton(props: {
  tabSignal: [Accessor<number>, Setter<number>];
}) {
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
          set(`${an}/config/tx/senders/${id}/channels`, 2);
          set(`${an}/config/tx/senders/${id}/autostart`, false);
          set(`${an}/config/tx/senders/${id}/packetTime`, 1);
          set(`${an}/config/tx/senders/${id}/sampleFormat`, "L24");
        }
        props.tabSignal[1](Number.MAX_SAFE_INTEGER);
      }}
    >
      +
    </button>
  );
}

const addReceiver = async (
  setTab: Setter<number>,
  name: string | null = null,
  channels: number = 2,
  autostart: boolean = false,
  sampleFormat: string = "L24",
  destinationIP?: string,
  destinationPort?: number,
) => {
  setCreateRcvSubmenuOpen(false);
  setSenderListOpen(false);
  const an = appName();
  console.log("Add receiver ...");
  let id = await locked(`${appName()}/config/rx/next-id`, async () => {
    const id = (await get<number>(`${appName()}/config/rx/next-id`)) || 1;
    set(`${an}/config/rx/next-id`, id + 1);
    return id;
  });
  if (id != null) {
    set(`${an}/config/rx/receivers/${id}/name`, name);
    set(`${an}/config/rx/receivers/${id}/channels`, channels);
    set(`${an}/config/rx/receivers/${id}/autostart`, autostart);
    set(`${an}/config/rx/receivers/${id}/sampleFormat`, sampleFormat);
    if (destinationIP != null) {
      set(`${an}/config/rx/receivers/${id}/sourceIP`, destinationIP);
    }
    if (destinationPort != null) {
      set(`${an}/config/rx/receivers/${id}/sourcePort`, destinationPort);
    }
    set(`${an}/config/rx/receivers/${id}/linkOffset`, 4);
    set(`${an}/config/rx/receivers/${id}/rtpOffset`, 0);
  }
  setTab(Number.MAX_SAFE_INTEGER);
};

const [createRcvSubmenuOpen, setCreateRcvSubmenuOpen] =
  createSignal<boolean>(false);
const [senderListOpen, setSenderListOpen] = createSignal<boolean>(false);

type SessionId = {
  id: number;
  version: number;
};
type SessionInfo = {
  id: SessionId;
  name: string;
  destinationIp: string;
  destinationPort: number;
  channels: number;
  sampleFormat: string;
  sampleRate: number;
  packetTime: number;
};

function SessionList(props: { tabSignal: [Accessor<number>, Setter<number>] }) {
  const [sessions, setSessions] = createSignal<SessionInfo[]>([]);

  pSubscribe<SessionInfo>(
    `${appName()}/discovery/sessions/?/config`,
    (sessionNames) => {
      const sessions = Array.from(sessionNames.values());
      setSessions(sessions);
    },
  );

  return (
    <div class="dropdown-menu" classList={{ open: senderListOpen() }}>
      <For each={sessions()}>
        {(session) => (
          <div
            class="menuitem"
            onclick={() => {
              addReceiver(
                props.tabSignal[1],
                session.name,
                session.channels,
                false,
                session.sampleFormat,
                session.destinationIp,
                session.destinationPort,
              );
            }}
          >
            {session.name}
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
        <div class="menuitem" onclick={() => addReceiver(props.tabSignal[1])}>
          Custom
        </div>
        <div
          class="menuitem submenu"
          on:mouseenter={() => setSenderListOpen(true)}
          on:mouseleave={() => setSenderListOpen(false)}
        >
          <span>From Sender</span>
          <span>🢒</span>
          <SessionList tabSignal={props.tabSignal} />
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
