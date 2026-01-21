import {
  createWbSignal,
  invalidChannels,
  invalidDestinationIP as invalidSourceIP,
  invalidDestinationPort as invalidSourcePort,
  invalidSampleFormat,
  transceiverID,
  invalidLinkOffset,
  invalidRtpOffset,
} from "../../utils";
import { pDelete, set } from "../../worterbuch";
import { appName } from "../../vscState";
import { createEffect, createSignal } from "solid-js";
import { IoPlay } from "solid-icons/io";
import { IoStop } from "solid-icons/io";
import { IoTrash } from "solid-icons/io";
import { createReceiver, deleteReceiver } from "../../api";

export default function Editor(props: { receiver: [string, string] }) {
  const [name, setName] = createWbSignal<string, string>(
    `/config/rx/receivers/${transceiverID(props.receiver)}/name`,
    props.receiver[1],
  );

  const [channels, setChannels] = createWbSignal<string, number>(
    `/config/rx/receivers/${transceiverID(props.receiver)}/channels`,
    "0",
    [(s) => parseInt(s, 10) || 0, (n) => n.toString()],
  );

  const [sampleFormat, setSampleFormat] = createWbSignal<string, string>(
    `/config/rx/receivers/${transceiverID(props.receiver)}/sampleFormat`,
    "L24",
  );

  const [sourceIP, setSourceIP] = createWbSignal<string, string>(
    `/config/rx/receivers/${transceiverID(props.receiver)}/sourceIP`,
    "",
  );

  const [sourcePort, setSourcePort] = createWbSignal<string, number>(
    `/config/rx/receivers/${transceiverID(props.receiver)}/sourcePort`,
    "0",
    [(s) => parseInt(s, 10) || 0, (n) => n.toString()],
  );

  const [linkOffset, setLinkOffset] = createWbSignal<string, number>(
    `/config/rx/receivers/${transceiverID(props.receiver)}/linkOffset`,
    "4",
    [(s) => parseInt(s, 10) || 0, (n) => n.toString()],
  );

  const [rtpOffset, setRtpOffset] = createWbSignal<string, number>(
    `/config/rx/receivers/${transceiverID(props.receiver)}/rtpOffset`,
    "0",
    [(s) => parseInt(s, 10) || 0, (n) => n.toString()],
  );

  const [vscRunning] = createWbSignal<boolean, boolean>(`/running`, false);

  const [configInvalid, setConfigInvalid] = createSignal<boolean>(false);
  createEffect(() => {
    const invalid =
      invalidChannels(channels()) ||
      invalidSampleFormat(sampleFormat()) ||
      invalidSourceIP(sourceIP()) ||
      invalidSourcePort(sourcePort());
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

  const updateSourceIP = (e: Event) => {
    const input = e.target as HTMLInputElement;
    const newSource = input.value;
    setSourceIP(newSource);
  };

  const updateSourcePort = (e: Event) => {
    const input = e.target as HTMLInputElement;
    const newSource = input.value;
    setSourcePort(newSource);
  };

  const updateLinkOffset = (e: Event) => {
    const input = e.target as HTMLInputElement;
    const newSource = input.value;
    setLinkOffset(newSource);
  };

  const updateRtpOffset = (e: Event) => {
    const input = e.target as HTMLInputElement;
    const newSource = input.value;
    setRtpOffset(newSource);
  };

  const [running] = createWbSignal<boolean, boolean>(
    `/rx/${transceiverID(props.receiver)}/running`,
    false,
  );

  const start = () => {
    console.log(`Starting receiver ${transceiverID(props.receiver)}...`);
    set(
      `${appName()}/config/rx/receivers/${transceiverID(
        props.receiver,
      )}/autostart`,
      true,
    );

    createReceiver(parseInt(transceiverID(props.receiver), 10)).catch((err) =>
      // TODO show error to user
      console.error("Failed to start receiver:", err),
    );
  };

  const stop = () => {
    console.log(`Stopping receiver ${transceiverID(props.receiver)}...`);
    set(
      `${appName()}/config/rx/receivers/${transceiverID(
        props.receiver,
      )}/autostart`,
      false,
    );

    deleteReceiver(parseInt(transceiverID(props.receiver), 10)).catch((err) =>
      // TODO show error to user
      console.error("Failed to stop receiver:", err),
    );
  };

  const startStop = () => {
    if (running()) {
      stop();
    } else {
      start();
    }
  };

  const deleteReceiverConfig = () => {
    // TODO show confirmation dialog
    pDelete(
      `${appName()}/config/rx/receivers/${transceiverID(props.receiver)}/#`,
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

      <label class="key" for="sourceIP">
        Source IP:
      </label>
      <input
        classList={{ invalid: invalidSourceIP(sourceIP()) }}
        id="sourceIP"
        type="text"
        inputmode="numeric"
        value={sourceIP()}
        onChange={updateSourceIP}
        disabled={running()}
      />

      <label class="key" for="sourcePort">
        Source Port:
      </label>
      <input
        classList={{ invalid: invalidSourcePort(sourcePort()) }}
        id="sourcePort"
        type="text"
        inputmode="numeric"
        value={sourcePort()}
        onChange={updateSourcePort}
        disabled={running()}
      />

      <label class="key" for="linkOffset">
        Link Offset:
      </label>
      <input
        classList={{ invalid: invalidLinkOffset(linkOffset()) }}
        id="linkOffset"
        type="text"
        inputmode="numeric"
        value={linkOffset()}
        onChange={updateLinkOffset}
        disabled={running()}
      />

      <label class="key" for="rtpOffset">
        RTP Offset:
      </label>
      <input
        classList={{ invalid: invalidRtpOffset(rtpOffset()) }}
        id="rtpOffset"
        type="text"
        inputmode="numeric"
        value={rtpOffset()}
        onChange={updateRtpOffset}
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
      <button id="delete" on:click={deleteReceiverConfig} disabled={running()}>
        <span class="icon-label">
          <IoTrash />
          Delete
        </span>
      </button>
    </div>
  );
}
