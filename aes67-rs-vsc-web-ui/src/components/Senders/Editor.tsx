import {
  createWbSignal,
  invalidChannels,
  invalidIP,
  invalidPacketTime,
  invalidPort,
  invalidSampleFormat,
  transceiverID,
} from "../../utils";
import { pDelete, set } from "../../worterbuch";
import { appName } from "../../vscState";
import { createEffect, createSignal } from "solid-js";
import { IoPlay } from "solid-icons/io";
import { IoStop } from "solid-icons/io";
import { IoTrash } from "solid-icons/io";
import { createSender, deleteSender } from "../../api";

export default function Editor(props: { sender: [string, string] }) {
  const [name, setName] = createWbSignal<string, string>(
    `/config/tx/senders/${transceiverID(props.sender)}/name`,
    props.sender[1],
  );

  const [channels, setChannels] = createWbSignal<string, number>(
    `/config/tx/senders/${transceiverID(props.sender)}/channels`,
    "0",
    [(s) => parseInt(s, 10) || 0, (n) => n.toString()],
  );

  const [sampleFormat, setSampleFormat] = createWbSignal<string, string>(
    `/config/tx/senders/${transceiverID(props.sender)}/sampleFormat`,
    "L24",
  );

  const [packetTime, setPacketTime] = createWbSignal<string, number>(
    `/config/tx/senders/${transceiverID(props.sender)}/packetTime`,
    "1",
    [(s) => parseFloat(s) || 1, (n) => n.toString()],
  );

  const [destinationIP, setDestinationIP] = createWbSignal<string, string>(
    `/config/tx/senders/${transceiverID(props.sender)}/destinationIP`,
    "",
  );

  const [destinationPort, setDestinationPort] = createWbSignal<string, number>(
    `/config/tx/senders/${transceiverID(props.sender)}/destinationPort`,
    "0",
    [(s) => parseInt(s, 10) || 0, (n) => n.toString()],
  );

  const [vscRunning] = createWbSignal<boolean, boolean>(`/running`, false);

  const [configInvalid, setConfigInvalid] = createSignal<boolean>(false);
  createEffect(() => {
    const invalid =
      invalidChannels(channels()) ||
      invalidSampleFormat(sampleFormat()) ||
      invalidIP(destinationIP()) ||
      invalidPort(destinationPort()) ||
      invalidPacketTime(packetTime());
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

  const updatePacketTime = (e: Event) => {
    const input = e.target as HTMLSelectElement;
    const newPacketTime = input.value;
    setPacketTime(newPacketTime || "1");
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

  const [running] = createWbSignal<boolean, boolean>(
    `/tx/${transceiverID(props.sender)}/running`,
    false,
  );

  const start = () => {
    console.log(`Starting sender ${transceiverID(props.sender)}...`);
    set(
      `${appName()}/config/tx/senders/${transceiverID(props.sender)}/autostart`,
      true,
    );

    createSender(parseInt(transceiverID(props.sender), 10)).catch((err) =>
      // TODO show error to user
      console.error(`Failed to start sender:`, err),
    );
  };

  const stop = () => {
    console.log(`Stopping sender ${transceiverID(props.sender)}...`);
    set(
      `${appName()}/config/tx/senders/${transceiverID(props.sender)}/autostart`,
      false,
    );

    deleteSender(parseInt(transceiverID(props.sender), 10)).catch((err) =>
      // TODO show error to user
      console.error("Failed to stop sender:", err),
    );
  };

  const startStop = () => {
    if (running()) {
      stop();
    } else {
      start();
    }
  };

  const deleteSenderConfig = () => {
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

      <label class="key" for="ptime">
        Packet Time (ms):
      </label>
      <select
        id="ptime"
        value={packetTime() || "1"}
        onChange={updatePacketTime}
        // disabled={running()}
      >
        <option value="4">4.0</option>
        <option value="2">2.0</option>
        <option value="1">1.0</option>
        <option value="0.25">0.25</option>
        <option value="0.125">0.125</option>
      </select>

      <label class="key" for="destinationIP">
        Destination IP:
      </label>
      <input
        classList={{ invalid: invalidIP(destinationIP()) }}
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
        classList={{ invalid: invalidPort(destinationPort()) }}
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
      <button id="delete" on:click={deleteSenderConfig} disabled={running()}>
        <span class="icon-label">
          <IoTrash />
          Delete
        </span>
      </button>
    </div>
  );
}
