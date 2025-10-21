export function transceiverID(key: string): string {
  const parts = key.split("/");
  return parts[parts.length - 2];
}

export function sortTransceivers(
  transceivers: [string, string][]
): [string, string][] {
  return transceivers.sort((a, b) => a[0].localeCompare(b[0]));
}
