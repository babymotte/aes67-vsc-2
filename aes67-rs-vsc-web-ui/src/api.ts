export async function fetchAppName(): Promise<string> {
  const url = "/api/v1/backend/app-name";

  const response = await fetch(url, {
    method: "GET",
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch app name: ${response.statusText}`);
  }

  return await response.text();
}

export async function stopVsc(): Promise<void> {
  const url = "/api/v1/vsc/stop";
  const response = await fetch(url, {
    method: "POST",
  });

  if (!response.ok) {
    throw new Error(`Failed to stop VSC: ${response.statusText}`);
  }
}

export async function startVsc(): Promise<void> {
  const url = "/api/v1/vsc/start";
  const response = await fetch(url, {
    method: "POST",
  });

  if (!response.ok) {
    throw new Error(`Failed to start VSC: ${response.statusText}`);
  }
}
