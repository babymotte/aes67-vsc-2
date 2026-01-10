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

export function createWbSignal<T extends Value>(
  path: string,
  defaultValue: T
): [Accessor<T>, (newValue: T) => void] {
  const [value, setValue] = createSignal<T>(defaultValue);

  createEffect(() => {
    subscribe<T>(path, (n) => {
      if (n.value) {
        const v = n.value;
        setValue((_) => v);
      } else {
        setValue((_) => defaultValue);
      }
    });
  });

  return [value, (newValue) => set(path, newValue)] as const;
}
