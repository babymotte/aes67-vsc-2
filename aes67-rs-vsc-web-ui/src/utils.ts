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
    return key.split("/")[4];
  } else {
    return name;
  }
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
