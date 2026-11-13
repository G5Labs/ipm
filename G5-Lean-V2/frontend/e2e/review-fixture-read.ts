/** A freshly running VM can briefly lose its host tunnel during fixture setup. */
export async function readReviewFixtureFile<
  T extends { status: number; body: unknown },
>(read: () => Promise<T>): Promise<T> {
  for (let attempt = 0; ; attempt++) {
    const response = await read();
    if (attempt === 10 || !isClosedHostTunnel(response)) return response;
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
}

function isClosedHostTunnel(response: {
  status: number;
  body: unknown;
}): boolean {
  if (response.status !== 503) return false;
  const body = response.body;
  if (!body || typeof body !== "object" || !("error" in body)) return false;
  const error = body.error;
  return (
    !!error &&
    typeof error === "object" &&
    "code" in error &&
    error.code === "SERVICE_UNAVAILABLE" &&
    "message" in error &&
    typeof error.message === "string" &&
    error.message.includes("HOST_TUNNEL_CLOSED")
  );
}
