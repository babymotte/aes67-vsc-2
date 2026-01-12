import type { Accessor } from "solid-js";
import "./Indicator.css";

export default function Indicator(props: {
  onLabel: string;
  offLabel: string;
  on: Accessor<boolean>;
}) {
  return (
    <div class="running-indicator" classList={{ running: props.on() }}>
      <span>{props.on() ? props.onLabel : props.offLabel}</span>
      <span class="indicator" classList={{ active: props.on() }}>
        {props.on() ? "🟢" : "🔴"}
      </span>
    </div>
  );
}
