import { running } from "../vscState";

export default function RunningIndicator() {
  return (
    <div class="running-indicator" classList={{ running: running() }}>
      <span>{running() ? "Running" : "Stopped"}</span>
      <span class="indicator" classList={{ active: running() }}>
        ●
      </span>
    </div>
  );
}
