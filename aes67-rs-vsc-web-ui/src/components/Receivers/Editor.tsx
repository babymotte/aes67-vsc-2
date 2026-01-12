import {
  createWbSignal,
  invalidChannels,
  invalidDestinationIP,
  invalidDestinationPort,
  invalidSampleFormat,
  transceiverID,
} from "../../utils";
import { pDelete, set, subscribe } from "../../worterbuch";
import { appName } from "../../vscState";
import { createEffect, createSignal } from "solid-js";
import { IoPlay } from "solid-icons/io";
import { IoStop } from "solid-icons/io";
import { IoTrash } from "solid-icons/io";

export default function Editor(props: { receiver: [string, string] }) {
  const [name, setName] = createWbSignal<string, string>(
    `${appName()}/config/rx/receivers/${transceiverID(props.receiver)}/name`,
    props.receiver[1]
  );

  const [channels, setChannels] = createWbSignal<string, number>(
    `${appName()}/config/rx/receivers/${transceiverID(
      props.receiver
    )}/channels`,
    "0",
    [(s) => parseInt(s, 10) || 0, (n) => n.toString()]
  );

  const [sampleFormat, setSampleFormat] = createWbSignal<string, string>(
    `${appName()}/config/rx/receivers/${transceiverID(
      props.receiver
    )}/sampleFormat`,
    "L24"
  );

  const [destinationIP, setDestinationIP] = createWbSignal<string, string>(
    `${appName()}/config/rx/receivers/${transceiverID(
      props.receiver
    )}/destinationIP`,
    ""
  );

  const [destinationPort, setDestinationPort] = createWbSignal<string, number>(
    `${appName()}/config/rx/receivers/${transceiverID(
      props.receiver
    )}/destinationPort`,
    "0",
    [(s) => parseInt(s, 10) || 0, (n) => n.toString()]
  );

  const [vscRunning] = createWbSignal<boolean, boolean>(
    `${appName()}/running`,
    false
  );

  const [configInvalid, setConfigInvalid] = createSignal<boolean>(false);
  createEffect(() => {
    const invalid =
      invalidChannels(channels()) ||
      invalidSampleFormat(sampleFormat()) ||
      invalidDestinationIP(destinationIP()) ||
      invalidDestinationPort(destinationPort());
    setConfigInvalid(invalid);
  });

  const updateName = (e: Event) => {
    const input = e.target as HTMLInputElement;
    const newName = input.value;
    setName(newName);
  };

  const updateChannels = (e: Event) => {
    const input = e.target as HTMLInputElement;
    const newChannels = input.value;
    setChannels(newChannels || "0");
  };

  const updateSampleFormat = (e: Event) => {
    const input = e.target as HTMLInputElement;
    const newSampleFormat = input.value;
    setSampleFormat(newSampleFormat || "L24");
  };

  const updateDestinationIP = (e: Event) => {
    const input = e.target as HTMLInputElement;
    const newDestination = input.value;
    setDestinationIP(newDestination);
  };

  const updateDestinationPort = (e: Event) => {
    const input = e.target as HTMLInputElement;
    const newDestination = input.value;
    setDestinationPort(newDestination);
  };

  const [running, setRunning] = createSignal<boolean>(false);
  createEffect(() => {
    subscribe<boolean>(
      `${appName()}/rx/${transceiverID(props.receiver)}/running`,
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
    console.log(`Starting receiver ${transceiverID(props.receiver)}...`);
    set(
      `${appName()}/config/rx/receivers/${transceiverID(
        props.receiver
      )}/autostart`,
      true
    );

    // TODO implement start receiver

    set(`${appName()}/rx/${transceiverID(props.receiver)}/running`, true);
  };

  const stop = () => {
    console.log(`Stopping receiver ${transceiverID(props.receiver)}...`);
    set(
      `${appName()}/config/rx/receivers/${transceiverID(
        props.receiver
      )}/autostart`,
      false
    );

    // TODO implement stop receiver

    set(`${appName()}/rx/${transceiverID(props.receiver)}/running`, false);
  };

  const startStop = () => {
    if (running()) {
      stop();
    } else {
      start();
    }
  };

  const deleteReceiver = () => {
    // TODO show confirmation dialog
    // TODO invoke delete API and only remove config if successful
    pDelete(
      `${appName()}/config/rx/receivers/${transceiverID(props.receiver)}/#`
    );
  };

  return (
    <div class="config-page">
      <h2>Receiver Configuration</h2>

      <h3>General</h3>

      <label class="key" for="name">
        Name:
      </label>
      <input
        id="name"
        type="text"
        value={name()}
        onChange={updateName}
        // disabled={running()}
      />

      <label class="key" for="channels">
        Channels:
      </label>
      <input
        classList={{ invalid: invalidChannels(channels()) }}
        id="channels"
        type="text"
        inputmode="numeric"
        value={channels() || "0"}
        onChange={updateChannels}
        disabled={running()}
      />

      <label class="key" for="sampleFormat">
        Bit Depth:
      </label>
      <select
        classList={{ invalid: invalidSampleFormat(sampleFormat()) }}
        id="sampleFormat"
        value={sampleFormat() || "0"}
        onChange={updateSampleFormat}
        disabled={running()}
      >
        <option value="L16">16 Bit</option>
        <option value="L24">24 Bit</option>
      </select>

      <label class="key" for="destinationIP">
        Destination IP:
      </label>
      <input
        classList={{ invalid: invalidDestinationIP(destinationIP()) }}
        id="destinationIP"
        type="text"
        inputmode="numeric"
        value={destinationIP()}
        onChange={updateDestinationIP}
        disabled={running()}
      />

      <label class="key" for="destinationPort">
        Destination Port:
      </label>
      <input
        classList={{ invalid: invalidDestinationPort(destinationPort()) }}
        id="destinationPort"
        type="text"
        inputmode="numeric"
        value={destinationPort()}
        onChange={updateDestinationPort}
        disabled={running()}
      />

      <div class="separator" />
      <button
        id="startStop"
        on:click={startStop}
        disabled={configInvalid() || !vscRunning()}
      >
        {vscRunning() && running() ? (
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
      <button id="delete" on:click={deleteReceiver} disabled={running()}>
        <span class="icon-label">
          <IoTrash />
          Delete
        </span>
      </button>
    </div>
  );
}
