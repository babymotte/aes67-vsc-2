import { createWbSignal, transceiverID } from "../../utils";
import { pDelete, set, subscribe } from "../../worterbuch";
import { appName } from "../../vscState";
import { createEffect, createSignal } from "solid-js";
import { IoPlay } from "solid-icons/io";
import { IoStop } from "solid-icons/io";
import { IoTrash } from "solid-icons/io";

export default function Editor(props: { sender: [string, string] }) {
  const [name, setName] = createWbSignal<string>(
    `${appName()}/config/tx/senders/${transceiverID(props.sender)}/name`,
    props.sender[1]
  );

  const [channels, setChannels] = createWbSignal<number>(
    `${appName()}/config/tx/senders/${transceiverID(props.sender)}/channels`,
    2
  );

  const updateName = (e: Event) => {
    const input = e.target as HTMLInputElement;
    const newName = input.value;
    setName(newName);
  };

  const updateChannels = (e: Event) => {
    const input = e.target as HTMLInputElement;
    const newChannels = parseInt(input.value, 10);
    setChannels(newChannels || 2);
  };

  const [running, setRunning] = createSignal<boolean>(false);
  createEffect(() => {
    subscribe<boolean>(
      `${appName()}/tx/${transceiverID(props.sender)}/running`,
      (n) => {
        if (n.value !== undefined) {
          setRunning(n.value);
        } else {
          setRunning(false);
        }
      }
    );
  });

  const start = () => {
    console.log(`Starting sender ${transceiverID(props.sender)}...`);
    set(
      `${appName()}/config/tx/senders/${transceiverID(props.sender)}/autostart`,
      true
    );

    // TODO implement start sender

    set(`${appName()}/tx/${transceiverID(props.sender)}/running`, true);
  };

  const stop = () => {
    console.log(`Stopping sender ${transceiverID(props.sender)}...`);
    set(
      `${appName()}/config/tx/senders/${transceiverID(props.sender)}/autostart`,
      false
    );

    // TODO implement stop sender

    set(`${appName()}/tx/${transceiverID(props.sender)}/running`, false);
  };

  const startStop = () => {
    if (running()) {
      stop();
    } else {
      start();
    }
  };

  const deleteSender = () => {
    // TODO show confirmation dialog
    // TODO invoke delete API and only remove config if successful
    pDelete(`${appName()}/config/tx/senders/${transceiverID(props.sender)}/#`);
  };

  return (
    <div class="config-page">
      <h2>Sender Configuration</h2>

      <h3>General</h3>
      <label class="key" for="name">
        Name:
      </label>
      <input id="name" type="text" value={name()} onChange={updateName} />
      <label class="key" for="channels">
        Channels:
      </label>
      <input
        id="channels"
        type="text"
        inputmode="numeric"
        value={channels()}
        onChange={updateChannels}
      />

      <div class="separator">---------------------------</div>
      <button id="startStop" on:click={startStop}>
        {running() ? (
          <span class="icon-label">
            <IoStop />
            Stop
          </span>
        ) : (
          <span class="icon-label">
            <IoPlay />
            Start
          </span>
        )}
      </button>
      <button id="delete" on:click={deleteSender} disabled={running()}>
        <span class="icon-label">
          <IoTrash />
          Delete
        </span>
      </button>
    </div>
  );
}
