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

export async function createSender(id: number): Promise<void> {
  const url = "/api/v1/vsc/tx/create";
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ id }),
  });

  if (!response.ok) {
    throw new Error(`Failed to create sender: ${response.statusText}`);
  }
}

export async function createReceiver(id: number): Promise<void> {
  const url = "/api/v1/vsc/rx/create";
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ id }),
  });

  if (!response.ok) {
    throw new Error(`Failed to create receiver: ${response.statusText}`);
  }
}

export async function updateSender(id: number): Promise<void> {
  const url = "/api/v1/vsc/tx/update";
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ id }),
  });

  if (!response.ok) {
    throw new Error(`Failed to update sender: ${response.statusText}`);
  }
}

export async function updateReceiver(id: number): Promise<void> {
  const url = "/api/v1/vsc/rx/update";
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ id }),
  });

  if (!response.ok) {
    throw new Error(`Failed to update receiver: ${response.statusText}`);
  }
}

export async function deleteSender(id: number): Promise<void> {
  const url = "/api/v1/vsc/tx/delete";
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ id }),
  });

  if (!response.ok) {
    throw new Error(`Failed to delete sender: ${response.statusText}`);
  }
}

export async function deleteReceiver(id: number): Promise<void> {
  const url = "/api/v1/vsc/rx/delete";
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ id }),
  });

  if (!response.ok) {
    throw new Error(`Failed to delete receiver: ${response.statusText}`);
  }
}
