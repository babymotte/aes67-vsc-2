import { appName, running } from "../../vscState";
import { createEffect, createSignal, For } from "solid-js";
import { set, subscribeLs } from "../../worterbuch";
import { startVsc, stopVsc } from "../../api";
import { IoPlay, IoStop } from "solid-icons/io";
import { createWbSignal } from "../../utils";

const collator = new Intl.Collator(undefined, {
  numeric: true,
  sensitivity: "base",
});

type PtpConfig = { nic: string | null };
type PtpMode = "system" | { phc: PtpConfig } | { internal: PtpConfig };

function nicForPtptMode(pm: PtpMode): string | null {
  if (typeof pm === "object" && "phc" in pm) {
    return pm.phc.nic;
  } else if (typeof pm === "object" && "internal" in pm) {
    return pm.internal.nic;
  }
  return null;
}

function valueForPtptMode(pm: PtpMode): string {
  if (typeof pm === "object" && "phc" in pm) {
    return "phc";
  } else if (typeof pm === "object" && "internal" in pm) {
    return "internal";
  }
  return "system";
}

function NetworkInterfaceOption(props: { nic: string }) {
  const [enabled] = createWbSignal<boolean, boolean>(
    `/networkInterfaces/${props.nic}/active`,
    false
  );
  const [ptpEnabled] = createWbSignal<boolean, boolean>(
    `/networkInterfaces/${props.nic}/ptp`,
    false
  );
  return (
    <option disabled={!enabled()} value={props.nic}>
      {ptpEnabled() ? `${props.nic} ⏲️` : props.nic}
    </option>
  );
}

export default function VSC() {
  const startStopVSC = async (running: boolean) => {
    set(`${appName()}/config/autostart`, !running);
    if (running) {
      stopVsc().catch((err) =>
        // TODO show error to user
        console.error("Failed to stop VSC:", err)
      );
    } else {
      startVsc().catch((err) =>
        // TODO show error to user
        console.error("Failed to start VSC:", err)
      );
    }
  };

  const [nics, setNics] = createSignal<string[]>([]);
  createEffect(() =>
    subscribeLs(`${appName()}/networkInterfaces`, (ch) =>
      setNics(ch.sort((a, b) => collator.compare(a, b)))
    )
  );

  const [audioNic, setAudioNic] = createWbSignal<string | null, string | null>(
    `/config/audio/nic`,
    null
  );
  const [ptpMode, setPtpMode] = createWbSignal<PtpMode, PtpMode>(
    `/config/ptp`,
    "system"
  );
  const [ptpNic, setPtpNic] = createSignal<string | null>(null);
  const [ptpNicSelectionDisabled, setPtpNicSelectionDisabled] =
    createSignal<boolean>(false);

  createEffect(() => {
    if (running()) {
      setPtpNicSelectionDisabled(true);
      return;
    }
    const pm = ptpMode();
    if (pm === "system") {
      setPtpNicSelectionDisabled(true);
    } else {
      setPtpNicSelectionDisabled(false);
    }
    const nic = nicForPtptMode(pm);
    if (nic) {
      setPtpNic(nic);
    }
  });

  const updateAudioNic = (e: Event) => {
    const select = e.target as HTMLSelectElement;
    const nic = select.value;
    setAudioNic(nic);
  };

  const updatePtpMode = (e: Event) => {
    const select = e.target as HTMLSelectElement;
    const mode = select.value;

    switch (mode) {
      case "system": {
        const pm = mode;
        setPtpMode(pm);
        break;
      }
      case "phc": {
        const pm = { phc: { nic: ptpNic() } };
        setPtpMode(pm);
        break;
      }
      case "internal": {
        const pm = { internal: { nic: ptpNic() } };
        setPtpMode(pm);
        break;
      }
    }
  };

  const updatePtpNic = (e: Event) => {
    const select = e.target as HTMLSelectElement;
    const nic = select.value;
    const pm = ptpMode();
    if (typeof pm === "object" && "phc" in pm) {
      setPtpMode({ phc: { nic } });
    } else if (typeof pm === "object" && "internal" in pm) {
      setPtpMode({ internal: { nic } });
    }
    setPtpNic(nic);
  };

  const [sampleRate, setSampleRate] = createWbSignal<string, number>(
    `/config/audio/sampleRate`,
    "48000",
    [(s) => parseInt(s), (v) => v.toString()]
  );
  const updateSampleRate = (e: Event) => {
    const select = e.target as HTMLSelectElement;
    setSampleRate(select.value);
  };

  return (
    <div class="config-page">
      <h2>VSC Configuration</h2>

      <h3>General</h3>

      <label class="key" for="start-stop-vsc">
        Run:
      </label>
      <button
        class="value"
        id="start-stop-vsc"
        on:click={() => startStopVSC(running())}
      >
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

      <h3>Audio over IP</h3>

      <label class="key" for="sample-rate">
        Sample Rate:
      </label>
      <select
        disabled={running()}
        id="sample-rate"
        on:change={updateSampleRate}
        value={sampleRate()}
      >
        <option value="44100">44.1 kHz</option>
        <option value="48000">48 kHz</option>
        <option value="96000">96 kHz</option>
      </select>

      <label class="key" for="audio-nic">
        Multicast Interface:
      </label>
      <select
        disabled={running()}
        id="audio-nic"
        on:change={updateAudioNic}
        value={audioNic() || undefined}
      >
        <For each={nics()}>{(nic) => <NetworkInterfaceOption nic={nic} />}</For>
      </select>

      <h3>PTP</h3>
      <label class="key" for="ptp-mode">
        Clock Source:
      </label>
      <select
        disabled={running()}
        id="ptp-mode"
        on:change={updatePtpMode}
        value={valueForPtptMode(ptpMode())}
      >
        <option value="system">System Clock</option>
        <option value="internal">Internal PTP Client</option>
        <option value="phc">Network Interface PHC</option>
      </select>

      <label class="key" for="ptp-nic">
        Network Interface:
      </label>
      <select
        disabled={ptpNicSelectionDisabled()}
        id="ptp-nic"
        on:change={updatePtpNic}
        value={ptpNic() || undefined}
      >
        <For each={nics()}>{(nic) => <NetworkInterfaceOption nic={nic} />}</For>
      </select>
    </div>
  );
}
