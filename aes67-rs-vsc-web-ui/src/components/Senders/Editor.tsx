import { createWbSignal, transceiverID } from "../../utils";
import { pDelete } from "../../worterbuch";
import { appName } from "../../vscState";

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

  const startStop = () => {
    // TODO implement start/stop sender
  };

  const deleteSender = () => {
    // TODO show confirmation dialog
    // TODO invoke delete API and only remove config if successful
    pDelete(`${appName()}/config/tx/senders/${transceiverID(props.sender)}/#`);
  };

  return (
    <div class="config-page">
      <h2>Sender Config</h2>

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
        Start
      </button>
      <button id="delete" on:click={deleteSender}>
        Delete
      </button>
    </div>
  );
}
