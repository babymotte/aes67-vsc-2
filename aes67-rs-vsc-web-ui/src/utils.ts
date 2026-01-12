import { createEffect, createSignal, type Accessor } from "solid-js";
import { set, subscribe } from "./worterbuch";
import type { Value } from "worterbuch-js";

const txCollator = new Intl.Collator(undefined, {
  numeric: true,
  sensitivity: "base",
});

const rxCollator = new Intl.Collator(undefined, {
  numeric: true,
  sensitivity: "base",
});

export function transceiverLabel([key, name]: string[]): string {
  if (name == null || name.trim() === "") {
    return transceiverID([key]);
  } else {
    return name;
  }
}

export function transceiverID([key]: string[]): string {
  return key.split("/")[4];
}

export function sortSenders(
  transceivers: [string, string][]
): [string, string][] {
  return transceivers.sort((a, b) => txCollator.compare(a[0], b[0]));
}

export function sortReceivers(
  transceivers: [string, string][]
): [string, string][] {
  return transceivers.sort((a, b) => rxCollator.compare(a[0], b[0]));
}

export function createWbSignal<T, V extends Value>(
  path: string,
  defaultValue: T,
  conversion?: [(to: T) => V, (from: V) => T]
): [Accessor<T>, (newValue: T) => void] {
  const [value, setValue] = createSignal<T>(defaultValue);

  createEffect(() => {
    subscribe<V>(
      path,
      (n) => {
        if (n.value) {
          const v = n.value;
          setValue((_) => (conversion ? conversion[1](v) : (v as T)));
        } else {
          setValue((_) => defaultValue);
        }
      },
      false
    );
  });

  return [
    value,
    (newValue) =>
      set(
        path,
        conversion ? conversion[0](newValue) : (newValue as unknown as V)
      ),
  ] as const;
}

export function invalidDestinationIP(value: string): boolean {
  return !/^((25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])\.){3}(25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])$/.test(
    value
  );
}

export function invalidDestinationPort(value: string): boolean {
  let port = parseInt(value, 10);
  return port < 1024 || port > 65535;
}

export function invalidChannels(value: string): boolean {
  let channels = parseInt(value, 10);
  return channels < 1 || channels > 64;
}

export function invalidPacketTime(value: string): boolean {
  let packetTime = parseFloat(value);
  return (
    packetTime != 4.0 &&
    packetTime != 2.0 &&
    packetTime != 1.0 &&
    packetTime != 0.25 &&
    packetTime != 0.125
  );
}

export function invalidSampleFormat(value: string): boolean {
  return value != "L16" && value != "L24";
}
