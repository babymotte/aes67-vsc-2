import { createWbSignal } from "../../utils";

export default function JACK() {
  const [bufferSize, setBufferSize] = createWbSignal<string, number>(
    `/config/jack/bufferSize`,
    "1024",
    [(s) => parseInt(s) || undefined, (v) => v.toString()],
  );

  const updateBufferSize = (e: Event) => {
    const select = e.target as HTMLSelectElement;
    setBufferSize(select.value);
  };

  return (
    <div class="config-page">
      <h2>JACK Configuration</h2>

      <h3>General</h3>

      <label class="key" for="buffer-size">
        Buffer Size:
      </label>
      <input
        type="text"
        class="value"
        id="buffer-size"
        value={bufferSize()}
        on:input={updateBufferSize}
      />
    </div>
  );
}
