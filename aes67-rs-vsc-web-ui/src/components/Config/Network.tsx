import { createEffect, createSignal } from "solid-js";
import Selection from "../Selection";
import { appName } from "../../vscState";
import { get, pSubscribe, set, subscribe } from "../../worterbuch";

export default function Network() {
  const [options, setOptions] = createSignal<[string, string, boolean][]>([]);

  const an = appName();

  pSubscribe<boolean>(`${an}/networkInterfaces/?/active`, (nics) => {
    let opts: [string, string, boolean][] = [...nics].map(([key, active]) => {
      const nic = key.split("/")[2];
      return [nic, nic, !active];
    });
    opts.sort((a, b) => a[1].localeCompare(b[1]));
    setOptions(opts);
  });

  const [audioNic, setAudioNic] = createSignal<string>("");
  const [ptpNic, setPtpNic] = createSignal<string>("");

  subscribe<string>(`${an}/config/audio/nic`, (nic) => {
    if (nic.value) {
      setAudioNic(nic.value);
    }
  });

  subscribe<string>(`${an}/config/ptp/nic`, (nic) => {
    if (nic.value) {
      setPtpNic(nic.value);
    }
  });

  const onAudioSelection = async (nic: string) => {
    if (nic != null && nic.trim() != "") {
      await set(`${an}/config/audio/nic`, nic);
    }
  };

  const onPtpSelection = async (nic: string) => {
    if (nic != null && nic.trim() != "") {
      await set(`${an}/config/ptp/nic`, nic);
    }
  };

  createEffect(async () => {
    setAudioNic((await get<string>(`${an}/config/audio/nic`)) || "");
    setPtpNic((await get<string>(`${an}/config/ptp/nic`)) || "");
  }, [options]);

  return (
    <>
      <h3>Network Configuration</h3>
      <div class="form">
        <h4>Audio over IP:</h4>
        <label for="audio-nic">Network Interface:</label>
        <Selection
          id="audio-nic"
          options={options}
          onSelection={onAudioSelection}
          value={audioNic}
        />
        <h4>PTP:</h4>
        <label for="ptp-nic">Network Interface:</label>
        <Selection
          id="ptp-nic"
          options={options}
          onSelection={onPtpSelection}
          value={ptpNic}
        />
      </div>
    </>
  );
}
