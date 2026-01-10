import { appName, running } from "../../vscState";
import { createEffect, createSignal } from "solid-js";
import Selection from "../Selection";
import { get, pSubscribe, set, subscribe } from "../../worterbuch";
import { startVsc, stopVsc } from "../../api";

type PtpConfig = { nic: string };
type PtpMode = "system" | { phc: PtpConfig } | { internal: PtpConfig };

function nicForPtptMode(pm: PtpMode): string | undefined {
  if (typeof pm === "object" && "phc" in pm) {
    return pm.phc.nic;
  } else if (typeof pm === "object" && "internal" in pm) {
    return pm.internal.nic;
  }
  return undefined;
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

  const [options, setOptions] = createSignal<[string, string, boolean][]>([]);
  const [ptpModeOptions] = createSignal<[string, string, boolean][]>([
    ["system", "System Clock", false],
    ["phc", "PHC", false],
    ["internal", "Internal", false],
  ]);

  createEffect(() => {
    pSubscribe<boolean>(`${appName()}/networkInterfaces/?/active`, (nics) => {
      let opts: [string, string, boolean][] = [...nics].map(([key, active]) => {
        const nic = key.split("/")[2];
        return [nic, nic, !active];
      });
      opts.sort((a, b) => a[1].localeCompare(b[1]));
      setOptions(opts);
    });
  });

  createEffect(() => {
    subscribe<string>(`${appName()}/config/audio/nic`, (nic) => {
      if (nic.value) {
        setAudioNic(nic.value);
      }
    });
  });

  createEffect(() => {
    subscribe<PtpMode>(`${appName()}/config/ptp`, (nic) => {
      if (nic.value) {
        setPtpMode(nic.value);
      }
    });
  });

  const [audioNic, setAudioNic] = createSignal<string>("");
  const [ptpNic, setPtpNic] = createSignal<string>("");
  const [ptpMode, setPtpMode] = createSignal<PtpMode>("system");
  const [ptpModeValue, setPtpModeValue] = createSignal<string>("");
  const [ptpNicSelectionDisabled, setPtpNicSelectionDisabled] =
    createSignal<boolean>(false);

  const onAudioSelection = async (nic: string) => {
    if (nic != null && nic.trim() != "") {
      await set(`${appName()}/config/audio/nic`, nic);
    }
  };

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
  });

  const onPtpModeSelection = async (mode: string) => {
    switch (mode) {
      case "system": {
        const pm = mode;
        await set(`${appName()}/config/ptp`, pm);
        setPtpMode(pm);
        break;
      }
      case "phc": {
        const pm = { phc: { nic: ptpNic() } };
        await set(`${appName()}/config/ptp`, pm);
        setPtpMode(pm);
        break;
      }
      case "internal": {
        const pm = { internal: { nic: ptpNic() } };
        await set(`${appName()}/config/ptp`, pm);
        setPtpMode(pm);
        break;
      }
    }
  };

  const onPtpSelection = async (nic: string) => {
    const pm = ptpMode();
    if (typeof pm === "object" && "phc" in pm) {
      await set(`${appName()}/config/ptp`, { phc: { nic } });
      setPtpMode({ phc: { nic } });
    } else if (typeof pm === "object" && "internal" in pm) {
      await set(`${appName()}/config/ptp`, { internal: { nic } });
      setPtpMode({ internal: { nic } });
    }
    setPtpNic(nic);
  };

  createEffect(async () => {
    setAudioNic((await get<string>(`${appName()}/config/audio/nic`)) || "");
    setPtpMode((await get<PtpMode>(`${appName()}/config/ptp`)) || "system");
    const nic = nicForPtptMode(ptpMode());
    if (nic) {
      setPtpNic(nic);
    }
  }, [options]);

  createEffect(() => {
    const pm = ptpMode();
    if (pm === "system") {
      setPtpModeValue("system");
    } else if (typeof pm === "object" && "phc" in pm) {
      setPtpModeValue("phc");
      setPtpNic(pm.phc.nic);
    } else if (typeof pm === "object" && "internal" in pm) {
      setPtpModeValue("internal");
      setPtpNic(pm.internal.nic);
    }
  });

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
        {running() ? "Stop" : "Start"}
      </button>

      <h3>Audio over IP</h3>
      <label class="key" for="audio-nic">
        Network Interface:
      </label>
      <Selection
        disabled={running}
        id="audio-nic"
        options={options}
        onSelection={onAudioSelection}
        value={audioNic}
      />

      <h3>PTP</h3>
      <label class="key" for="ptp-mode">
        Mode:
      </label>
      <Selection
        disabled={running}
        id="ptp-mode"
        options={ptpModeOptions}
        onSelection={onPtpModeSelection}
        value={ptpModeValue}
      />
      <label class="key" for="ptp-nic">
        Network Interface:
      </label>
      <Selection
        disabled={ptpNicSelectionDisabled}
        id="ptp-nic"
        options={options}
        onSelection={onPtpSelection}
        value={ptpNic}
      />
    </div>
  );
}
